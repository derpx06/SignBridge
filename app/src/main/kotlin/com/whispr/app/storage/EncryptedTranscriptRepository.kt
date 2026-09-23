package com.whispr.app.storage

import android.content.Context
import androidx.security.crypto.EncryptedFile
import androidx.security.crypto.MasterKey
import com.whispr.core.FinalTranscriptSegment
import com.whispr.core.SavedSession
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.security.MessageDigest
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption

/**
 * Production transcript persistence using AndroidX Crypto in application-private storage.
 * Only completed [SavedSession] values cross this boundary, so raw audio cannot be written here.
 */
class EncryptedTranscriptRepository(context: Context) : TranscriptRepository {
    private val appContext = context.applicationContext
    private val directory = File(appContext.filesDir, "encrypted-transcripts")
    private val masterKey = MasterKey.Builder(appContext).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build()

    override fun save(session: SavedSession) {
        directory.mkdirs()
        val target = fileFor(session.id)
        val temporary = File(directory, ".${fileKey(session.id)}.tmp")
        encrypted(temporary).openFileOutput().use { output ->
            DataOutputStream(output).use { SessionBinaryCodec.write(it, session) }
        }
        try {
            try {
                Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            } catch (_: AtomicMoveNotSupportedException) {
                Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING)
            }
        } catch (failure: Exception) {
            temporary.delete()
            throw IllegalStateException("Could not install encrypted transcript ${session.id}", failure)
        }
    }

    override fun list(): List<SavedSession> = directory.listFiles { file -> file.name.endsWith(".session") }
        ?.mapNotNull { file ->
            readIgnoringCorruption { read(file) } ?: run {
                // A truncated/old-key file must not crash startup forever.
                file.delete()
                null
            }
        }
        ?.sortedByDescending(SavedSession::completedAtMs)
        ?: emptyList()

    override fun get(id: String): SavedSession? = fileFor(id).takeIf(File::exists)?.let { file ->
        readIgnoringCorruption { read(file) } ?: run { file.delete(); null }
    }

    override fun delete(id: String) {
        val target = fileFor(id)
        if (target.exists() && !target.delete()) throw IllegalStateException("Could not delete transcript $id")
    }

    private fun read(file: File): SavedSession = encrypted(file).openFileInput().use { input ->
        DataInputStream(input).use(SessionBinaryCodec::read)
    }

    private fun encrypted(file: File) = EncryptedFile.Builder(
        appContext,
        file,
        masterKey,
        EncryptedFile.FileEncryptionScheme.AES256_GCM_HKDF_4KB,
    ).build()

    private fun fileFor(id: String) = File(directory, "${fileKey(id)}.session")

    private fun fileKey(id: String): String = MessageDigest.getInstance("SHA-256")
        .digest(id.toByteArray(Charsets.UTF_8))
        .joinToString("") { "%02x".format(it) }
}

/** Keeps malformed or undecryptable legacy files from taking down the app at startup. */
internal fun <T> readIgnoringCorruption(read: () -> T): T? = try {
    read()
} catch (_: Exception) {
    null
}

private object SessionBinaryCodec {
    fun write(output: DataOutputStream, session: SavedSession) {
        output.writeString(session.id)
        output.writeLong(session.startedAtMs)
        output.writeLong(session.stoppedAtMs)
        output.writeLong(session.completedAtMs)
        output.writeInt(session.segments.size)
        session.segments.forEach { segment ->
            output.writeString(segment.id)
            output.writeString(segment.text)
            output.writeLong(segment.startMs)
            output.writeLong(segment.endMs)
            output.writeBoolean(segment.speakerId != null)
            segment.speakerId?.let { speakerId -> output.writeString(speakerId) }
        }
    }

    fun read(input: DataInputStream): SavedSession = SavedSession(
        id = input.readString(),
        startedAtMs = input.readLong(),
        stoppedAtMs = input.readLong(),
        completedAtMs = input.readLong(),
        segments = List(input.readInt()) {
            FinalTranscriptSegment(
                id = input.readString(),
                text = input.readString(),
                startMs = input.readLong(),
                endMs = input.readLong(),
                speakerId = input.readStringOrNull(),
            )
        },
    )

    private fun DataOutputStream.writeString(value: String) {
        val bytes = value.toByteArray(Charsets.UTF_8)
        writeInt(bytes.size)
        write(bytes)
    }

    private fun DataInputStream.readString(): String {
        val size = readInt()
        require(size >= 0 && size <= MAX_STRING_BYTES) { "Invalid encrypted transcript string length" }
        return ByteArray(size).also(::readFully).toString(Charsets.UTF_8)
    }

    private fun DataInputStream.readStringOrNull(): String? = if (readBoolean()) readString() else null

    private const val MAX_STRING_BYTES = 10 * 1024 * 1024
}
