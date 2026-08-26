package com.cosync.mobile

import android.content.Context
import android.net.wifi.WifiManager
import androidx.core.content.ContextCompat
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.modules.core.DeviceEventManagerModule
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import uniffi.cosync_mobile.ConnectionClient

class CosyncBridgeModule(private val context: ReactApplicationContext) : ReactContextBaseJavaModule(context) {
  private val executor: ScheduledExecutorService = Executors.newSingleThreadScheduledExecutor()
  private val client by lazy { ConnectionClient(context.filesDir.resolve("cosync").absolutePath) }
  private var multicastLock: WifiManager.MulticastLock? = null
  private var lastStatus: Boolean? = null

  override fun getName(): String = "Cosync"

  @ReactMethod
  fun pair(payload: String, deviceName: String, promise: Promise) {
    executor.execute {
      try {
        acquireMulticastLock()
        val result = client.pair(payload, deviceName)
        if (result == "connected") {
          ContextCompat.startForegroundService(
            context,
            CosyncForegroundService.intent(context)
          )
          startStatusPolling()
        } else {
          releaseMulticastLock()
        }
        promise.resolve(result)
        emitStatus(result == "connected")
      } catch (error: Exception) {
        releaseMulticastLock()
        promise.reject("PAIRING_FAILED", error.message, error)
      }
    }
  }

  @ReactMethod
  fun isConnected(promise: Promise) {
    executor.execute { promise.resolve(client.isConnected()) }
  }

  @ReactMethod
  fun disconnect(promise: Promise) {
    executor.execute {
      client.disconnect()
      releaseMulticastLock()
      context.stopService(CosyncForegroundService.intent(context))
      emitStatus(false)
      promise.resolve(null)
    }
  }

  override fun invalidate() {
    client.disconnect()
    releaseMulticastLock()
    executor.shutdownNow()
    super.invalidate()
  }

  private fun startStatusPolling() {
    executor.scheduleAtFixedRate({ emitStatus(client.isConnected()) }, 0, 1, TimeUnit.SECONDS)
  }

  private fun emitStatus(connected: Boolean) {
    if (lastStatus == connected) return
    lastStatus = connected
    context
      .getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java)
      .emit("cosyncConnectionState", connected)
  }

  private fun acquireMulticastLock() {
    if (multicastLock?.isHeld == true) return
    val manager = context.getSystemService(Context.WIFI_SERVICE) as WifiManager
    multicastLock = manager.createMulticastLock("cosync-discovery").apply {
      setReferenceCounted(false)
      acquire()
    }
  }

  private fun releaseMulticastLock() {
    multicastLock?.let { if (it.isHeld) it.release() }
    multicastLock = null
  }
}
