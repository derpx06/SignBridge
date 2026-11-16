package com.whispr.core.edit

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

class GgufTieTest {
    @get:Rule val tmp = TemporaryFolder()

    private class T(val name: String, val data: ByteArray)

    /** A tiny valid GGUF v3 file: a string, an int, a string array, and three tensors. */
    private fun build(tensors: List<T>, alignment: Int = 32): ByteArray {
        fun le(n: Int) = ByteBuffer.allocate(n).order(ByteOrder.LITTLE_ENDIAN)
        fun str(s: String) = le(8 + s.length).apply { putLong(s.length.toLong()); put(s.toByteArray()) }.array()
        val kv = ByteArrayOutputStream()
        fun kvString(k: String, v: String) { kv.write(str(k)); kv.write(le(4).putInt(8).array()); kv.write(str(v)) }
        fun kvU32(k: String, v: Int) { kv.write(str(k)); kv.write(le(4).putInt(4).array()); kv.write(le(4).putInt(v).array()) }
        kvString("general.architecture", "qwen3")
        kvU32("general.alignment", alignment)
        kv.write(str("tokenizer.tokens")); kv.write(le(4).putInt(9).array()); kv.write(le(4).putInt(8).array()); kv.write(le(8).putLong(3).array())
        listOf("a", "bb", "ccc").forEach { kv.write(str(it)) }
        var offset = 0L
        val infos = ByteArrayOutputStream()
        val data = ByteArrayOutputStream()
        for (t in tensors) {
            infos.write(str(t.name)); infos.write(le(4).putInt(1).array()); infos.write(le(8).putLong(t.data.size.toLong()).array())
            infos.write(le(4).putInt(0).array()); infos.write(le(8).putLong(offset).array())
            data.write(t.data)
            val pad = ((alignment - t.data.size % alignment) % alignment)
            data.write(ByteArray(pad))
            offset += t.data.size + pad
        }
        val head = le(24).putInt(0x46554747).putInt(3).putLong(tensors.size.toLong()).putLong(3).array()
        val out = ByteArrayOutputStream()
        out.write(head); out.write(kv.toByteArray()); out.write(infos.toByteArray())
        val padTo = ((alignment - out.size() % alignment) % alignment)
        out.write(ByteArray(padTo)); out.write(data.toByteArray())
        return out.toByteArray()
    }

    private fun tensors() = listOf(
        T("token_embd.weight", ByteArray(100) { (it + 1).toByte() }),
        T("output.weight", ByteArray(70) { (200 - it).toByte() }),
        T("blk.0.attn_q.weight", ByteArray(33) { (it * 3).toByte() }),
    )

    /** Reads back every tensor's bytes through the file's own table. */
    private fun readTensors(f: ByteArray): Map<String, ByteArray> {
        val b = ByteBuffer.wrap(f).order(ByteOrder.LITTLE_ENDIAN)
        b.int; b.int; val n = b.long; val kvCount = b.long
        fun str(): String { val l = b.long.toInt(); val a = ByteArray(l); b.get(a); return String(a) }
        var align = 32
        repeat(kvCount.toInt()) {
            val k = str(); val t = b.int
            when (t) { 8 -> str(); 4 -> { val v = b.int; if (k == "general.alignment") align = v }
                9 -> { val e = b.int; val c = b.long; repeat(c.toInt()) { if (e == 8) str() } } }
        }
        val names = ArrayList<String>(); val offs = ArrayList<Long>(); val lens = ArrayList<Long>()
        repeat(n.toInt()) { names += str(); b.int; lens += b.long; b.int; offs += b.long }
        val dataStart = (b.position() + align - 1) / align * align
        return names.indices.associate { names[it] to f.copyOfRange(dataStart + offs[it].toInt(), dataStart + offs[it].toInt() + lens[it].toInt()) }
    }

    @Test fun dropsTheTensorAndKeepsTheRestByteForByte() {
        val src = tmp.newFile("m.gguf"); src.writeBytes(build(tensors()))
        val dst = File(tmp.root, "tied.gguf")
        assertTrue(GgufTie.dropTensor(src, dst))
        val out = readTensors(dst.readBytes())
        assertEquals(setOf("token_embd.weight", "blk.0.attn_q.weight"), out.keys)
        assertArrayEquals(tensors()[0].data, out.getValue("token_embd.weight"))
        assertArrayEquals(tensors()[2].data, out.getValue("blk.0.attn_q.weight"))
        assertTrue(dst.length() < src.length())
    }

    @Test fun worksWithOtherAlignments() {
        val src = tmp.newFile("m64.gguf"); src.writeBytes(build(tensors(), alignment = 64))
        val dst = File(tmp.root, "tied64.gguf")
        assertTrue(GgufTie.dropTensor(src, dst))
        assertArrayEquals(tensors()[2].data, readTensors(dst.readBytes()).getValue("blk.0.attn_q.weight"))
    }

    @Test fun missingTensorWritesNothing() {
        val src = tmp.newFile("m2.gguf"); src.writeBytes(build(tensors()))
        val dst = File(tmp.root, "none.gguf")
        assertFalse(GgufTie.dropTensor(src, dst, "nope.weight"))
        assertFalse(dst.exists())
    }

    @Test fun rejectsAFileThatIsNotGguf() {
        val src = tmp.newFile("x.gguf"); src.writeText("hello world, definitely not a model")
        try { GgufTie.dropTensor(src, File(tmp.root, "y.gguf")); throw AssertionError("should have thrown") } catch (e: java.io.IOException) { }
    }

    /** The real S1-mini, if it is on this machine: the Kotlin result must match the file made with the gguf Python package. */
    @Test fun realModelMatchesThePythonRewrite() {
        val real = File(System.getProperty("user.home"), ".cache/whispr-voice-lab/s1-mini/s1-mini-q4_k_m.gguf")
        if (!real.exists()) return
        val dst = File(tmp.root, "real-tied.gguf")
        assertTrue(GgufTie.dropTensor(real, dst))
        assertEquals(356_593_504L, dst.length()) // size of the Python-written reference
    }
}
