package com.whispr.app.storage

import androidx.room.Dao
import androidx.room.Database
import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.RoomDatabase
import androidx.room.Transaction
import com.whispr.core.FinalTranscriptSegment
import com.whispr.core.SavedSession
import java.time.Instant
import java.time.format.DateTimeFormatter

/** Persistence boundary for completed transcripts. It deliberately has no audio API. */
interface TranscriptRepository {
    fun save(session: SavedSession)
    fun list(): List<SavedSession>
    fun get(id: String): SavedSession?
    fun delete(id: String)
}

/** JVM-friendly fake used by repository clients and unit tests. */
class InMemoryTranscriptRepository : TranscriptRepository {
    private val sessions = linkedMapOf<String, SavedSession>()

    override fun save(session: SavedSession) {
        sessions[session.id] = session.copy(segments = session.segments.toList())
    }

    override fun list(): List<SavedSession> = sessions.values
        .sortedByDescending(SavedSession::completedAtMs)
        .map { it.copy(segments = it.segments.toList()) }

    override fun get(id: String): SavedSession? = sessions[id]?.let { it.copy(segments = it.segments.toList()) }

    override fun delete(id: String) {
        sessions.remove(id)
    }
}

@Entity(tableName = "transcript_sessions")
data class TranscriptSessionEntity(
    @PrimaryKey val id: String,
    val startedAtMs: Long,
    val stoppedAtMs: Long,
    val completedAtMs: Long,
)

@Entity(
    tableName = "transcript_segments",
    foreignKeys = [
        ForeignKey(
            entity = TranscriptSessionEntity::class,
            parentColumns = ["id"],
            childColumns = ["sessionId"],
            onDelete = ForeignKey.CASCADE,
        ),
    ],
    indices = [Index("sessionId")],
)
data class TranscriptSegmentEntity(
    @PrimaryKey val id: String,
    val sessionId: String,
    val position: Int,
    val text: String,
    val startMs: Long,
    val endMs: Long,
    val speakerId: String?,
)

data class TranscriptSessionWithSegments(
    val session: TranscriptSessionEntity,
    val segments: List<TranscriptSegmentEntity>,
)

@Dao
interface TranscriptSessionDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    fun insertSession(session: TranscriptSessionEntity)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    fun insertSegments(segments: List<TranscriptSegmentEntity>)

    @Query("DELETE FROM transcript_segments WHERE sessionId = :sessionId")
    fun deleteSegments(sessionId: String)

    @Query("DELETE FROM transcript_sessions WHERE id = :sessionId")
    fun deleteSession(sessionId: String)

    @Query("SELECT * FROM transcript_sessions ORDER BY completedAtMs DESC")
    fun sessionsNewestFirst(): List<TranscriptSessionEntity>

    @Query("SELECT * FROM transcript_segments WHERE sessionId = :sessionId ORDER BY position ASC")
    fun segmentsFor(sessionId: String): List<TranscriptSegmentEntity>

    @Query("SELECT * FROM transcript_sessions WHERE id = :sessionId LIMIT 1")
    fun session(sessionId: String): TranscriptSessionEntity?

    @Transaction
    fun replace(session: TranscriptSessionEntity, segments: List<TranscriptSegmentEntity>) {
        insertSession(session)
        deleteSegments(session.id)
        insertSegments(segments)
    }
}

@Database(entities = [TranscriptSessionEntity::class, TranscriptSegmentEntity::class], version = 1, exportSchema = false)
abstract class TranscriptDatabase : RoomDatabase() {
    abstract fun transcriptSessionDao(): TranscriptSessionDao
}

/** Room-backed repository; a separate encrypted repository is the production transcript store. */
internal class RoomTranscriptRepository(private val dao: TranscriptSessionDao) : TranscriptRepository {
    override fun save(session: SavedSession) {
        dao.replace(
            TranscriptSessionEntity(session.id, session.startedAtMs, session.stoppedAtMs, session.completedAtMs),
            session.segments.mapIndexed { position, segment -> segment.toEntity(session.id, position) },
        )
    }

    override fun list(): List<SavedSession> = dao.sessionsNewestFirst().map { entity ->
        entity.toSavedSession(dao.segmentsFor(entity.id))
    }

    override fun get(id: String): SavedSession? = dao.session(id)?.let { entity ->
        entity.toSavedSession(dao.segmentsFor(id))
    }

    override fun delete(id: String) = dao.deleteSession(id)

    private fun FinalTranscriptSegment.toEntity(sessionId: String, position: Int) = TranscriptSegmentEntity(
        id = "$sessionId:$id",
        sessionId = sessionId,
        position = position,
        text = text,
        startMs = startMs,
        endMs = endMs,
        speakerId = speakerId,
    )

    private fun TranscriptSessionEntity.toSavedSession(segments: List<TranscriptSegmentEntity>) = SavedSession(
        id = id,
        startedAtMs = startedAtMs,
        stoppedAtMs = stoppedAtMs,
        completedAtMs = completedAtMs,
        segments = segments.map { segment ->
            FinalTranscriptSegment(
                id = segment.id.removePrefix("$id:"),
                text = segment.text,
                startMs = segment.startMs,
                endMs = segment.endMs,
                speakerId = segment.speakerId,
            )
        },
    )
}

data class ShareTextPayload(val mimeType: String = "text/plain", val text: String)

class PlainTextTranscriptExporter {
    fun export(title: String, session: SavedSession): String = buildString {
        appendLine(title)
        appendLine()
        appendLine("Started: ${timestamp(session.startedAtMs)}")
        appendLine("Stopped: ${timestamp(session.stoppedAtMs)}")
        appendLine("Completed: ${timestamp(session.completedAtMs)}")
        appendLine()
        session.segments.forEach { segment ->
            appendLine("[${segment.startMs}–${segment.endMs} ms] ${segment.speakerId ?: "Unknown speaker"}: ${segment.text}")
        }
    }.trimEnd()

    fun sharePayload(title: String, session: SavedSession) = ShareTextPayload(text = export(title, session))

    private fun timestamp(milliseconds: Long): String = DateTimeFormatter.ISO_INSTANT.format(Instant.ofEpochMilli(milliseconds))
}
