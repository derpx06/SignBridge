package com.whispr.app

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.whispr.app.storage.AppPrivateModelPackStorage
import com.whispr.app.storage.DefaultModelPackManifest
import com.whispr.app.storage.ModelPackManager
import com.whispr.app.storage.ModelPackState
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class ModelDownloadController(context: Context) {
    private val descriptor = DefaultModelPackManifest.descriptors.first { it.id == "asr" }
    private val manager = ModelPackManager(AppPrivateModelPackStorage(context.applicationContext))
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    var state: ModelPackState by mutableStateOf(manager.stateFor(descriptor))
        private set

    val isReady: Boolean get() = state is ModelPackState.Ready

    fun download() {
        if (state is ModelPackState.Downloading) return
        state = ModelPackState.Downloading(descriptor)
        scope.launch {
            val result = try {
                val connection = (URL(descriptor.configurationUrl).openConnection() as HttpURLConnection).apply {
                    connectTimeout = 20_000
                    readTimeout = 120_000
                    instanceFollowRedirects = true
                }
                connection.connect()
                if (connection.responseCode !in 200..299) error("Model server returned HTTP ${connection.responseCode}")
                connection.inputStream.use { manager.installDownloadedZip(descriptor, it) }
            } catch (failure: Exception) {
                ModelPackState.Error(descriptor, failure.message ?: "Model download failed")
            }
            withContext(Dispatchers.Main) { state = result }
        }
    }
}
