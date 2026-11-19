package dev.touchtwin.app

import android.app.Application
import android.os.Build
import android.provider.Settings
import android.util.Log
import dev.touchtwin.core.PadEvent
import dev.touchtwin.core.PadListener
import dev.touchtwin.core.TouchTwin
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** What the screen shows. */
data class Status(
    val started: Boolean = false,
    val startError: String? = null,
    /** Paired laptops (names), empty until the first pairing. */
    val laptops: List<String> = emptyList(),
    /** Name of the connected laptop, or null. */
    val connectedTo: String? = null,
    /** The laptop's virtual touchpad is ready. */
    val padReady: Boolean = false,
    /** Why the laptop could not create its touchpad. */
    val padError: String? = null,
)

/**
 * Owns the Rust core for the whole process, so rotating the screen or reopening the app
 * keeps the connection.
 */
class TouchTwinApp : Application() {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val _status = MutableStateFlow(Status())
    val status: StateFlow<Status> = _status

    @Volatile
    var core: TouchTwin? = null
        private set

    override fun onCreate() {
        super.onCreate()
        scope.launch { start() }
    }

    private fun deviceName(): String =
        Settings.Global.getString(contentResolver, Settings.Global.DEVICE_NAME)
            ?: "${Build.MANUFACTURER} ${Build.MODEL}"

    private fun start() {
        try {
            val c = TouchTwin.start(filesDir.absolutePath, deviceName(), object : PadListener {
                override fun onEvent(event: PadEvent) = handle(event)
            })
            core = c
            _status.update { it.copy(started = true, laptops = c.laptops().map { l -> l.name }) }
        } catch (e: Exception) {
            Log.e("TouchTwin", "core failed to start", e)
            _status.update { it.copy(startError = e.message ?: e.toString()) }
        }
    }

    private fun handle(event: PadEvent) {
        val c = core
        when (event) {
            is PadEvent.Paired -> _status.update {
                it.copy(laptops = c?.laptops()?.map { l -> l.name } ?: (it.laptops + event.name))
            }
            is PadEvent.Connected -> _status.update {
                it.copy(connectedTo = event.name, padReady = false, padError = null)
            }
            is PadEvent.Disconnected -> _status.update {
                it.copy(connectedTo = null, padReady = false)
            }
            is PadEvent.PadReady -> _status.update { it.copy(padReady = true, padError = null) }
            is PadEvent.PadFailed -> _status.update {
                it.copy(padReady = false, padError = event.message)
            }
        }
    }

    /** Blocks: call from Dispatchers.IO. Returns an error message, or null on success. */
    fun pair(uri: String): String? {
        val c = core ?: return "TouchTwin is still starting"
        return try {
            c.pair(uri)
            _status.update { it.copy(laptops = c.laptops().map { l -> l.name }) }
            null
        } catch (e: Exception) {
            e.message ?: e.toString()
        }
    }

    fun forgetAll() {
        val c = core ?: return
        c.laptops().forEach { runCatching { c.unpair(it.id) } }
        _status.update { Status(started = true) }
    }
}
