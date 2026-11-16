package com.whispr.app.cloud

import android.content.Context
import com.whispr.app.data.CloudKeyStore
import com.whispr.app.flow.FlowSettings
import com.whispr.core.cloud.CloudApi
import com.whispr.core.cloud.CloudException
import com.whispr.core.cloud.CloudHttp
import com.whispr.core.cloud.CloudLive
import com.whispr.core.cloud.CloudProvider
import com.whispr.core.cloud.CloudProviders
import com.whispr.core.cloud.HttpReq
import com.whispr.core.cloud.HttpResp

/** The only code that sends audio off the phone. Nothing here logs the key, the URL or the audio. */
object CloudClient {
    /** What a dictation sends to: provider, key, model and (Azure) region. */
    class Setup(val provider: CloudProvider, val key: String, val model: String, val region: String) {
        override fun toString() = "Setup(${provider.id})"
    }

    /** The cloud setup to use, or null unless cloud is on (always / fallback), a provider is chosen and consented to, and its key is saved. */
    fun configured(context: Context, settings: FlowSettings): Setup? {
        if (settings.cloudMode == "off") return null
        val p = CloudProviders.byId(settings.cloudProvider) ?: return null
        if (!settings.hasCloudConsent(p.id)) return null
        val key = CloudKeyStore.get(context, p.id) ?: return null
        return Setup(p, key, settings.cloudModel.takeIf { it in p.models } ?: p.defaultModel, settings.cloudRegion)
    }

    /**
     * A started live (streaming) session, or null unless cloud is configured, the provider has a live API and its
     * Live toggle is on. [onText] gets the words so far (on OkHttp's thread). Never throws: a failure to start
     * means "no live words", the batch/on-device path still runs.
     */
    fun liveFor(context: Context, settings: FlowSettings, language: String? = null, onText: (String) -> Unit): CloudLive? {
        val s = configured(context, settings) ?: return null
        if (!s.provider.liveSupported || !settings.isCloudLive(s.provider.id)) return null
        return runCatching { CloudLive(s.provider, s.key, settings.cloudModel, s.region, language, onText) }.getOrNull()
    }

    /** Blocking. Throws [CloudException]; callers must treat every failure as "use the on-device text". */
    fun transcribe(setup: Setup, pcm: ShortArray, language: String? = null, cancelled: () -> Boolean = { false }): String =
        CloudApi.transcribe(setup.provider, setup.key, setup.model, setup.region, pcm, language, ::send, cancelled)

    /** The real network ([CloudHttp]: deadline sized to the audio, cut on cancel, Retry-After passed back). */
    private fun send(req: HttpReq): HttpResp = CloudHttp.send(req)

    /** Sends one second of silence. Null = the key was accepted; otherwise a short reason. */
    fun test(provider: CloudProvider, key: String, model: String = provider.defaultModel, region: String = ""): String? = try {
        CloudApi.transcribe(provider, key, model, region, ShortArray(16_000), null, ::send); null
    } catch (e: CloudException) {
        if (CloudApi.keyWorks(e)) null else e.kind.message // a 4xx about silent audio still proves the key works
    } catch (e: Throwable) { "Test failed" }
}
