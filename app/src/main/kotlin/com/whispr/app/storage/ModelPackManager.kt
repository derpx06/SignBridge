package com.whispr.app.storage

import android.content.Context
import java.io.BufferedInputStream
import java.io.InputStream
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import java.util.zip.ZipInputStream

data class ModelPackDescriptor(
    val id: String,
    val version: String,
    /** App configuration only. It is never a transcript or audio destination. */
    val configurationUrl: String,
    val sha256: String,
    val archiveType: ArchiveType = ArchiveType.ZIP,
)

enum class ArchiveType { ZIP, RAW_ARCHIVE }

object DefaultModelPackManifest {
    /** Official multilingual Whisper archive, downloaded into app-private storage. */
    val descriptors = listOf(
        ModelPackDescriptor(
            id = "asr",
            version = "whisper-tiny",
            configurationUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-tiny.tar.bz2",
            // HTTPS protects transport; a signed digest can be supplied in a catalog update.
            sha256 = "",
            archiveType = ArchiveType.RAW_ARCHIVE,
        ),
        ModelPackDescriptor("diarization", "", "", ""),
    )
}

sealed interface ModelPackState {
    data object Absent : ModelPackState
    data class Downloading(val descriptor: ModelPackDescriptor) : ModelPackState
    data class Ready(val descriptor: ModelPackDescriptor) : ModelPackState
    data class Error(val descriptor: ModelPackDescriptor, val message: String) : ModelPackState
}

/** File operations are isolated so model installation remains JVM-testable without Android services. */
interface ModelPackStorage {
    fun temporaryZip(descriptor: ModelPackDescriptor): Path
    fun stagingDirectory(descriptor: ModelPackDescriptor): Path
    fun installedDirectory(descriptor: ModelPackDescriptor): Path
    fun installedIdentity(descriptor: ModelPackDescriptor): String?
    fun replaceInstalledAtomically(descriptor: ModelPackDescriptor, stagingDirectory: Path)
    fun deleteRecursively(path: Path)
}

/** App-private storage adapter: callers pass a directory under Context.filesDir. */
class FileModelPackStorage(private val root: Path) : ModelPackStorage {
    override fun temporaryZip(descriptor: ModelPackDescriptor): Path {
        val directory = root.resolve("temporary")
        Files.createDirectories(directory)
        return Files.createTempFile(directory, "${descriptor.id}-", ".zip.part")
    }

    override fun stagingDirectory(descriptor: ModelPackDescriptor): Path {
        val directory = root.resolve("staging")
        Files.createDirectories(directory)
        return Files.createTempDirectory(directory, "${descriptor.id}-")
    }

    override fun installedDirectory(descriptor: ModelPackDescriptor): Path = root.resolve("installed").resolve(descriptor.id)

    override fun installedIdentity(descriptor: ModelPackDescriptor): String? {
        val marker = installedDirectory(descriptor).resolve(".whispr-pack")
        return marker.takeIf(Files::exists)?.let { Files.newBufferedReader(it).use { reader -> reader.readText() } }
    }

    override fun replaceInstalledAtomically(descriptor: ModelPackDescriptor, stagingDirectory: Path) {
        val installed = installedDirectory(descriptor)
        Files.createDirectories(installed.parent)
        Files.newBufferedWriter(stagingDirectory.resolve(".whispr-pack")).use { writer -> writer.write(identity(descriptor)) }
        val backup = installed.resolveSibling(".${installed.fileName}.backup")
        deleteRecursively(backup)
        val hadInstalled = Files.exists(installed)
        if (hadInstalled) move(installed, backup)
        try {
            move(stagingDirectory, installed)
            deleteRecursively(backup)
        } catch (failure: Exception) {
            if (hadInstalled && Files.exists(backup) && !Files.exists(installed)) move(backup, installed)
            throw failure
        }
    }

    override fun deleteRecursively(path: Path) {
        if (!Files.exists(path)) return
        Files.walk(path).use { paths -> paths.sorted(Comparator.reverseOrder()).forEach(Files::deleteIfExists) }
        path.parent?.let { parent -> if (Files.isDirectory(parent) && Files.list(parent).use { !it.findAny().isPresent }) Files.delete(parent) }
    }

    fun readInstalledText(descriptor: ModelPackDescriptor, relativePath: String): String =
        Files.newInputStream(installedDirectory(descriptor).resolve(relativePath)).bufferedReader().use { it.readText() }

    private fun move(source: Path, target: Path) {
        try {
            Files.move(source, target, StandardCopyOption.ATOMIC_MOVE)
        } catch (_: AtomicMoveNotSupportedException) {
            Files.move(source, target)
        }
    }

    private fun identity(descriptor: ModelPackDescriptor): String =
        "${descriptor.id}\n${descriptor.version}\n${descriptor.sha256.lowercase()}\n"
}

/** Android composition adapter that confines installed model packs to application-private filesDir. */
class AppPrivateModelPackStorage(context: Context) : ModelPackStorage by FileModelPackStorage(
    context.applicationContext.filesDir.toPath().resolve("model-packs"),
)

class ModelPackManager(private val storage: ModelPackStorage) {
    private val states = mutableMapOf<String, ModelPackState>()

    fun stateFor(descriptor: ModelPackDescriptor): ModelPackState =
        states[descriptor.id] ?: if (isConfigured(descriptor) && Files.exists(storage.installedDirectory(descriptor)) &&
            storage.installedIdentity(descriptor) == identity(descriptor)) {
            ModelPackState.Ready(descriptor)
        } else ModelPackState.Absent

    /** Installs a ZIP supplied by a downloader boundary; this component performs no networking. */
    fun installDownloadedZip(descriptor: ModelPackDescriptor, downloadedZip: InputStream): ModelPackState {
        if (!isConfigured(descriptor)) {
            return ModelPackState.Error(descriptor, "Model pack is not configured for this build").also { states[descriptor.id] = it }
        }
        states[descriptor.id] = ModelPackState.Downloading(descriptor)
        var temporaryZip: Path? = null
        var stagingDirectory: Path? = null
        return try {
            temporaryZip = storage.temporaryZip(descriptor)
            val actualSha256 = downloadedZip.copyToAndHash(temporaryZip)
            if (descriptor.sha256.isNotBlank()) {
                require(actualSha256.equals(descriptor.sha256, ignoreCase = true)) { "Model pack integrity check failed" }
            }

            stagingDirectory = storage.stagingDirectory(descriptor)
            if (descriptor.archiveType == ArchiveType.ZIP) unzipSafely(temporaryZip, stagingDirectory)
            else Files.copy(temporaryZip, stagingDirectory.resolve("model.pack"))
            storage.replaceInstalledAtomically(descriptor, stagingDirectory)
            stagingDirectory = null
            ModelPackState.Ready(descriptor).also { states[descriptor.id] = it }
        } catch (failure: Exception) {
            ModelPackState.Error(descriptor, failure.message ?: "Model pack installation failed").also { states[descriptor.id] = it }
        } finally {
            temporaryZip?.let(storage::deleteRecursively)
            stagingDirectory?.let(storage::deleteRecursively)
        }
    }

    private fun isConfigured(descriptor: ModelPackDescriptor): Boolean =
        descriptor.id.isNotBlank() && descriptor.version.isNotBlank() &&
            descriptor.configurationUrl.startsWith("https://") &&
            descriptor.sha256.isBlank() || descriptor.sha256.matches(Regex("[0-9a-fA-F]{64}"))

    private fun identity(descriptor: ModelPackDescriptor): String =
        "${descriptor.id}\n${descriptor.version}\n${descriptor.sha256.lowercase()}\n"

    private fun InputStream.copyToAndHash(destination: Path): String {
        val digest = MessageDigest.getInstance("SHA-256")
        use { input ->
            Files.newOutputStream(destination).use { output ->
                val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    output.write(buffer, 0, count)
                    digest.update(buffer, 0, count)
                }
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    private fun unzipSafely(zipPath: Path, stagingDirectory: Path) {
        ZipInputStream(BufferedInputStream(Files.newInputStream(zipPath))).use { zip ->
            while (true) {
                val entry = zip.nextEntry ?: break
                val output = stagingDirectory.resolve(entry.name).normalize()
                require(output.startsWith(stagingDirectory) && output != stagingDirectory) { "ZIP entry escapes model pack directory" }
                if (entry.isDirectory) {
                    Files.createDirectories(output)
                } else {
                    Files.createDirectories(output.parent)
                    Files.newOutputStream(output).use { destination -> zip.copyTo(destination) }
                }
                zip.closeEntry()
            }
        }
    }
}

fun sha256(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(bytes)
    .joinToString("") { "%02x".format(it) }
