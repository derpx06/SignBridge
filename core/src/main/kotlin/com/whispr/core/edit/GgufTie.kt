package com.whispr.core.edit

import java.io.File
import java.io.IOException
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.channels.FileChannel

/**
 * Makes a model's tied embeddings share one table, which saves about 127 MB of RAM and storage on
 * S1-mini / Qwen3-0.6B: the GGUF stores a Q6_K `output.weight` next to the Q4_K `token_embd.weight`,
 * but the model is tied, so llama.cpp can use `token_embd.weight` for the output projection when
 * `output.weight` is absent.
 *
 * The file is rewritten without that tensor. It is streamed (a small buffer, never the whole model in
 * memory): the header is parsed and rewritten, and the tensor data is copied with `transferTo`.
 */
object GgufTie {
    private const val MAGIC = 0x46554747 // "GGUF" little endian
    private const val DEFAULT_ALIGNMENT = 32L
    private const val HEADER = 24 // magic, version, tensor count, key/value count

    private class Info(val name: String, val raw: ByteArray, val offset: Long)

    /** Writes [dst] without the tensor called [name]. Returns false if [src] has no such tensor (then [dst] is not written). */
    fun dropTensor(src: File, dst: File, name: String = "output.weight"): Boolean {
        RandomAccessFile(src, "r").use { input ->
            val ch = input.channel
            val head = ByteBuffer.allocate(HEADER).order(ByteOrder.LITTLE_ENDIAN)
            ch.read(head, 0); head.flip()
            if (head.int != MAGIC) throw IOException("not a GGUF file")
            val version = head.int
            if (version < 2) throw IOException("GGUF version $version is not supported")
            val tensorCount = head.long
            val kvCount = head.long

            val r = Reader(ch, HEADER.toLong())
            // key/value section: copied verbatim; only parsed to find where it ends and the alignment
            var alignment = DEFAULT_ALIGNMENT
            repeat(kvCount.toInt()) {
                val key = r.string()
                val type = r.u32()
                if (key == "general.alignment" && type == 4) alignment = r.u32().toLong().let { it } else r.skipValue(type)
            }
            val kvEnd = r.pos
            val infos = ArrayList<Info>(tensorCount.toInt())
            repeat(tensorCount.toInt()) {
                val start = r.pos
                val tn = r.string()
                val dims = r.u32()
                repeat(dims) { r.u64() }
                r.u32() // type
                val offset = r.u64()
                infos += Info(tn, r.bytes(start, r.pos), offset)
            }
            val infoEnd = r.pos
            val dataStart = align(infoEnd, alignment)
            val dropIndex = infos.indexOfFirst { it.name == name }
            if (dropIndex < 0) return false

            // each tensor's stored size = distance to the next tensor (they are alignment-padded already)
            val order = infos.indices.sortedBy { infos[it].offset }
            val size = LongArray(infos.size)
            for ((k, idx) in order.withIndex()) {
                val end = if (k + 1 < order.size) infos[order[k + 1]].offset else ch.size() - dataStart
                size[idx] = end - infos[idx].offset
            }

            // new tensor table: same order, offsets re-based after removing the dropped chunk
            val keptOrder = order.filter { it != dropIndex }
            val newOffset = LongArray(infos.size)
            var cursor = 0L
            for (idx in keptOrder) { newOffset[idx] = cursor; cursor += size[idx] }

            val tmp = File(dst.parentFile, dst.name + ".part")
            RandomAccessFile(tmp, "rw").use { out ->
                out.setLength(0)
                val oc = out.channel
                val header = ByteBuffer.allocate(HEADER).order(ByteOrder.LITTLE_ENDIAN)
                header.putInt(MAGIC).putInt(version).putLong(tensorCount - 1).putLong(kvCount).flip()
                oc.write(header)
                copy(ch, HEADER.toLong(), kvEnd - HEADER, oc)
                var written = kvEnd
                for ((i, info) in infos.withIndex()) {
                    if (i == dropIndex) continue
                    val raw = info.raw.copyOf()
                    // the offset is the last 8 bytes of the entry
                    ByteBuffer.wrap(raw, raw.size - 8, 8).order(ByteOrder.LITTLE_ENDIAN).putLong(newOffset[i])
                    oc.write(ByteBuffer.wrap(raw))
                    written += raw.size
                }
                val newDataStart = align(written, alignment)
                oc.write(ByteBuffer.allocate((newDataStart - written).toInt()))
                for (idx in keptOrder) copy(ch, dataStart + infos[idx].offset, size[idx], oc)
            }
            if (dst.exists()) dst.delete()
            if (!tmp.renameTo(dst)) throw IOException("could not move the rewritten model into place")
            return true
        }
    }

    private fun align(v: Long, a: Long) = (v + a - 1) / a * a

    private fun copy(from: FileChannel, pos: Long, count: Long, to: FileChannel) {
        var done = 0L
        while (done < count) {
            val n = from.transferTo(pos + done, minOf(count - done, 8L shl 20), to)
            if (n <= 0) throw IOException("short copy")
            done += n
        }
    }

    /** Sequential little-endian reader over the header; buffers in 64 KB pieces. */
    private class Reader(private val ch: FileChannel, start: Long) {
        var pos = start
        private var buf = ByteBuffer.allocate(0)
        private var bufStart = start

        private fun ensure(n: Int) {
            if (pos >= bufStart && pos + n <= bufStart + buf.limit()) return
            val size = maxOf(n, 1 shl 16)
            val b = ByteBuffer.allocate(size).order(ByteOrder.LITTLE_ENDIAN)
            ch.read(b, pos); b.flip()
            if (b.limit() < n) throw IOException("truncated GGUF header")
            buf = b; bufStart = pos
        }
        private fun at(): Int = (pos - bufStart).toInt()
        fun u32(): Int { ensure(4); val v = buf.getInt(at()); pos += 4; return v }
        fun u64(): Long { ensure(8); val v = buf.getLong(at()); pos += 8; return v }
        fun u8(): Int { ensure(1); val v = buf.get(at()).toInt() and 0xFF; pos += 1; return v }
        fun string(): String {
            val len = u64().toInt()
            ensure(len.coerceAtLeast(1))
            val bytes = ByteArray(len); for (i in 0 until len) bytes[i] = buf.get(at() + i)
            pos += len
            return String(bytes, Charsets.UTF_8)
        }
        fun bytes(from: Long, to: Long): ByteArray {
            val out = ByteArray((to - from).toInt())
            val b = ByteBuffer.wrap(out)
            ch.read(b, from)
            return out
        }
        fun skip(n: Long) { pos += n }
        fun skipValue(type: Int) {
            when (type) {
                0, 1, 7 -> skip(1)
                2, 3 -> skip(2)
                4, 5, 6 -> skip(4)
                10, 11, 12 -> skip(8)
                8 -> { val len = u64(); skip(len) }
                9 -> {
                    val elem = u32(); val n = u64()
                    when (elem) {
                        0, 1, 7 -> skip(n)
                        2, 3 -> skip(2 * n)
                        4, 5, 6 -> skip(4 * n)
                        10, 11, 12 -> skip(8 * n)
                        else -> for (i in 0 until n) skipValue(elem)
                    }
                }
                else -> throw IOException("unknown GGUF value type $type")
            }
        }
    }
}
