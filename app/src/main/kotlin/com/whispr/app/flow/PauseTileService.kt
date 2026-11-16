package com.whispr.app.flow

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import com.whispr.app.R

/**
 * Quick Settings tile "Voicio": on = the bubble's accessibility service is running. Tapping it while on
 * pauses Voicio for real (disableSelf), so banking and payment apps stop warning. Tapping it while off
 * opens the service's Settings page, where one switch turns it back on (Android lets only the user do that).
 */
class PauseTileService : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        val tile = qsTile ?: return
        val on = FlowAccessibilityService.instance != null
        tile.state = if (on) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = getString(R.string.tile_label)
        tile.subtitle = getString(if (on) R.string.tile_on else R.string.tile_paused)
        tile.contentDescription = getString(if (on) R.string.tile_on_description else R.string.tile_paused_description)
        tile.updateTile()
    }

    // The Intent overload only runs below API 34 (guarded above), where it is the only one that exists.
    @SuppressLint("StartActivityAndCollapseDeprecated")
    override fun onClick() {
        super.onClick()
        if (FlowAccessibilityService.pause()) { onStartListening(); return }
        val intent = FlowAccessibilityService.turnBackOnIntent(this)
        if (Build.VERSION.SDK_INT >= 34) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
        } else {
            @Suppress("DEPRECATION") startActivityAndCollapse(intent)
        }
    }

    companion object {
        /** Ask the system to re-read the tile (after the service starts or stops). */
        fun refresh(context: Context) {
            runCatching { requestListeningState(context, ComponentName(context, PauseTileService::class.java)) }
        }
    }
}
