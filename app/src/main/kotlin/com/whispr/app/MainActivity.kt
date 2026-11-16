package com.whispr.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import com.whispr.app.flow.FlowSettings
import com.whispr.app.ui.C
import com.whispr.app.ui.Lattice
import com.whispr.app.ui.MainShell
import com.whispr.app.ui.Motion
import com.whispr.app.ui.Onboarding
import com.whispr.app.ui.S
import com.whispr.app.ui.StatusBarIcons
import com.whispr.app.ui.T
import com.whispr.app.ui.VoicioTheme
import kotlinx.coroutines.delay

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Light-only app on warm paper: dark status-bar and navigation-bar icons everywhere.
        val clear = android.graphics.Color.TRANSPARENT
        enableEdgeToEdge(statusBarStyle = SystemBarStyle.light(clear, clear), navigationBarStyle = SystemBarStyle.light(clear, clear))
        val settings = FlowSettings(this)
        setContent { VoicioTheme { AppRoot(settings) } }
    }
}

private enum class Stage { Welcome, App }

/** The system splash (themes, Android 12+) hands over straight to the welcome or the app: no second splash. */
@Composable
private fun AppRoot(settings: FlowSettings) {
    var onboarded by rememberSaveable { mutableStateOf(settings.onboarded) }
    val stage = if (onboarded) Stage.App else Stage.Welcome
    // On tablets and in landscape the app stays a readable column in the middle.
    Box(Modifier.fillMaxSize().background(C.Bg), contentAlignment = Alignment.TopCenter) {
        Crossfade(stage, animationSpec = tween(Motion.ENTER_MS, easing = Motion.Ease), label = "stage") {
            when (it) {
                Stage.Welcome -> Column(Modifier.widthIn(max = 840.dp).fillMaxHeight()) { Onboarding(settings) { onboarded = true } }
                Stage.App -> Box(Modifier.widthIn(max = 640.dp).fillMaxHeight()) { MainShell(settings, onReplayWelcome = { settings.onboarded = false; onboarded = false }) }
            }
        }
    }
}
