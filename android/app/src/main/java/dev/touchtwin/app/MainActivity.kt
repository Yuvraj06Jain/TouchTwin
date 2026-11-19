package dev.touchtwin.app

import android.net.wifi.WifiManager
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : ComponentActivity() {
    private val app get() = application as TouchTwinApp
    private var multicastLock: WifiManager.MulticastLock? = null

    /** Last known pad size, re-sent when the app comes back to the foreground. */
    private var padMm: Pair<Float, Float>? = null

    private var pairing by mutableStateOf<String?>(null) // progress / error text

    private val scan = registerForActivityResult(ScanContract()) { result ->
        val uri = result.contents ?: return@registerForActivityResult
        pairing = "Pairing…"
        app.scope.launch {
            val err = app.pair(uri)
            withContext(Dispatchers.Main) { pairing = err?.let { "Pairing failed: $it" } }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                val status by app.status.collectAsStateWithLifecycle()
                when {
                    status.startError != null -> Message("TouchTwin could not start:\n${status.startError}")
                    !status.started -> Message("Starting…")
                    status.laptops.isEmpty() -> PairScreen(pairing, ::startScan)
                    else -> PadScreen(status)
                }
            }
        }
    }

    override fun onStart() {
        super.onStart()
        hideSystemBars()
        val wifi = applicationContext.getSystemService(WifiManager::class.java)
        multicastLock = wifi?.createMulticastLock("touchtwin-mdns")?.apply {
            setReferenceCounted(false)
            acquire()
        }
        app.core?.reconnectNow()
        padMm?.let { (w, h) -> app.core?.openPad(w, h) }
    }

    override fun onStop() {
        app.core?.closePad()
        multicastLock?.takeIf { it.isHeld }?.release()
        super.onStop()
    }

    private fun hideSystemBars() {
        WindowInsetsControllerCompat(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
    }

    private fun startScan() {
        scan.launch(
            ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("Scan the code shown by `touchtwind pair` on your laptop")
                .setBeepEnabled(false)
                .setOrientationLocked(true)
        )
    }

    @Composable
    private fun Message(text: String) {
        Box(Modifier.fillMaxSize().background(Color(0xFF15171C)), contentAlignment = Alignment.Center) {
            Text(text, color = Color.White, textAlign = TextAlign.Center, modifier = Modifier.padding(32.dp))
        }
    }

    @Composable
    private fun PairScreen(progress: String?, onScan: () -> Unit) {
        Column(
            Modifier.fillMaxSize().background(Color(0xFF15171C)).padding(32.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("TouchTwin", color = Color.White, fontSize = 32.sp)
            Spacer(Modifier.size(12.dp))
            Text(
                "Your phone becomes your laptop's touchpad, with the same taps, scrolling and gestures.\n\n" +
                    "On the laptop, run:\n  touchtwind pair\nthen scan the code it shows.",
                color = Color(0xFFB8BCC8),
                textAlign = TextAlign.Center,
            )
            Spacer(Modifier.size(24.dp))
            Button(onClick = onScan, enabled = progress != "Pairing…") { Text("Scan pairing code") }
            progress?.let {
                Spacer(Modifier.size(16.dp))
                Text(it, color = Color(0xFFFFB4AB), textAlign = TextAlign.Center)
            }
        }
    }

    @Composable
    private fun PadScreen(status: Status) {
        Box(Modifier.fillMaxSize()) {
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { ctx ->
                    TouchPadView(
                        ctx,
                        onFingers = { app.core?.touch(it) },
                        onSize = { w, h ->
                            padMm = w to h
                            app.core?.openPad(w, h)
                        },
                    )
                },
            )
            StatusBar(status, Modifier.align(Alignment.TopCenter))
        }
    }

    @Composable
    private fun StatusBar(status: Status, modifier: Modifier) {
        var latency by remember { mutableStateOf<Int?>(null) }
        var menu by remember { mutableStateOf(false) }
        LaunchedEffect(status.connectedTo) {
            while (status.connectedTo != null) {
                latency = withContext(Dispatchers.IO) { app.core?.latencyMs()?.toInt() }
                delay(1000)
            }
            latency = null
        }
        val (dot, text) = when {
            status.padError != null -> Color(0xFFFF6B5E) to status.padError
            status.connectedTo == null -> Color(0xFFFFB020) to "Looking for ${status.laptops.first()}…"
            !status.padReady -> Color(0xFFFFB020) to "Connecting to ${status.connectedTo}…"
            else -> Color(0xFF3DDC84) to (status.connectedTo + (latency?.let { "  ·  $it ms" } ?: ""))
        }
        Row(
            modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(8.dp).background(dot, CircleShape))
            Spacer(Modifier.width(8.dp))
            Text(text, color = Color(0x99FFFFFF), fontSize = 12.sp, modifier = Modifier.weight(1f), maxLines = 3)
            Box {
                TextButton(onClick = { menu = true }) { Text("⋯", color = Color(0x99FFFFFF)) }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(text = { Text("Pair another laptop") }, onClick = { menu = false; startScan() })
                    DropdownMenuItem(text = { Text("Forget paired laptops") }, onClick = {
                        menu = false
                        app.scope.launch { app.forgetAll() }
                    })
                }
            }
        }
    }
}
