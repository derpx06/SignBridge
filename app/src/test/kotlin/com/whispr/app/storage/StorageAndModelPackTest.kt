package com.whispr.app.storage

import com.whispr.core.FinalTranscriptSegment
import com.whispr.core.SavedSession
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.nio.file.Files
import java.nio.file.Path
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StorageAndModelPackTest {
    private val temporaryRoots = mutableListOf<Path>()

    @After
    fun deleteTemporaryRoots() {
        temporaryRoots.forEach { root ->
            Files.walk(root).use { paths ->
                paths.sorted(Comparator.reverseOrder()).forEach(Files::deleteIfExists)
            }
        }
    }

    @Test
    fun `repository round trips transcript segments in their original order and lists newest first`() {
        val repository = InMemoryTranscriptRepository()
        val older = session(id = "older", completedAtMs = 10, segmentIds = listOf("first", "second"))
        val newer = session(id = "newer", completedAtMs = 20, segmentIds = listOf("only"))

        repository.save(older)
        repository.save(newer)

        assertEquals(listOf("newer", "older"), repository.list().map { it.id })
        assertEquals(listOf("first", "second"), repository.get("older")!!.segments.map { it.id })
    }

    @Test
    fun `plain text export includes title timestamps speaker labels and transcript text`() {
        val exported = PlainTextTranscriptExporter().export(
            title = "Weekly research sync",
            session = SavedSession(
                id = "session-1",
                startedAtMs = 0,
                stoppedAtMs = 1_000,
                completedAtMs = 2_000,
                segments = listOf(
                    FinalTranscriptSegment("one", "Good morning", 0, 750, "Ava"),
                    FinalTranscriptSegment("two", "Status update", 750, 1_000, null),
                ),
            ),
        )

        assertTrue(exported.contains("Weekly research sync"))
        assertTrue(exported.contains("Started: 1970-01-01T00:00:00Z"))
        assertTrue(exported.contains("Stopped: 1970-01-01T00:00:01Z"))
        assertTrue(exported.contains("Completed: 1970-01-01T00:00:02Z"))
        assertTrue(exported.contains("Ava: Good morning"))
        assertTrue(exported.contains("Unknown speaker: Status update"))
    }

    @Test
    fun `integrity mismatch leaves neither an installed pack nor temporary files`() {
        val root = temporaryRoot()
        val manager = ModelPackManager(FileModelPackStorage(root))
        val descriptor = ModelPackDescriptor("asr", "1", "https://models.invalid/asr.zip", sha256 = "00")

        val state = manager.installDownloadedZip(descriptor, ByteArrayInputStream(zipOf("model.bin" to "model")))

        assertTrue(state is ModelPackState.Error)
        assertFalse(Files.exists(root.resolve("installed/asr")))
        assertFalse(Files.exists(root.resolve("temporary")))
    }

    @Test
    fun `zip traversal failure leaves neither an installed pack nor staging files`() {
        val root = temporaryRoot()
        val zip = zipOf("../escape.bin" to "not allowed")
        val manager = ModelPackManager(FileModelPackStorage(root))
        val descriptor = ModelPackDescriptor("asr", "1", "https://models.invalid/asr.zip", sha256 = sha256(zip))

        val state = manager.installDownloadedZip(descriptor, ByteArrayInputStream(zip))

        assertTrue(state is ModelPackState.Error)
        assertFalse(Files.exists(root.resolve("installed/asr")))
        assertFalse(Files.exists(root.resolve("staging")))
        assertFalse(Files.exists(root.resolve("escape.bin")))
    }

    @Test
    fun `verified model pack transitions from absent to ready after atomic installation`() {
        val root = temporaryRoot()
        val zip = zipOf("models/acoustic.bin" to "verified bytes")
        val storage = FileModelPackStorage(root)
        val manager = ModelPackManager(storage)
        val descriptor = ModelPackDescriptor("asr", "1", "https://models.invalid/asr.zip", sha256 = sha256(zip))

        assertEquals(ModelPackState.Absent, manager.stateFor(descriptor))
        assertEquals(ModelPackState.Ready(descriptor), manager.installDownloadedZip(descriptor, ByteArrayInputStream(zip)))
        assertEquals("verified bytes", storage.readInstalledText(descriptor, "models/acoustic.bin"))
        assertEquals(ModelPackState.Ready(descriptor), manager.stateFor(descriptor))
    }

    @Test
    fun `default manifest provides separate configurable ASR and diarization descriptors`() {
        assertEquals(setOf("asr", "diarization"), DefaultModelPackManifest.descriptors.map { it.id }.toSet())
        assertTrue(DefaultModelPackManifest.descriptors.first { it.id == "asr" }.configurationUrl.startsWith("https://"))
        assertTrue(DefaultModelPackManifest.descriptors.first { it.id == "asr" }.archiveType == ArchiveType.RAW_ARCHIVE)
    }

    private fun session(id: String, completedAtMs: Long, segmentIds: List<String>) = SavedSession(
        id = id,
        startedAtMs = 0,
        stoppedAtMs = completedAtMs - 1,
        completedAtMs = completedAtMs,
        segments = segmentIds.mapIndexed { index, segmentId ->
            FinalTranscriptSegment(segmentId, "text-$segmentId", index * 100L, index * 100L + 50, "Speaker $index")
        },
    )

    private fun temporaryRoot(): Path = Files.createTempDirectory("whispr-task-2-").also(temporaryRoots::add)

    private fun zipOf(vararg entries: Pair<String, String>): ByteArray = ByteArrayOutputStream().use { output ->
        ZipOutputStream(output).use { zip ->
            entries.forEach { (name, contents) ->
                zip.putNextEntry(ZipEntry(name))
                zip.write(contents.toByteArray())
                zip.closeEntry()
            }
        }
        output.toByteArray()
    }
}
