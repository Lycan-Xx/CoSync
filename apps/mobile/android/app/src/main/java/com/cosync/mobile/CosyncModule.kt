package com.cosync.mobile

import android.content.Context
import android.net.wifi.WifiManager
import androidx.core.content.ContextCompat
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.modules.core.DeviceEventManagerModule
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicLong
import uniffi.cosync_mobile.ConnectionClient

class CosyncBridgeModule(private val context: ReactApplicationContext) : ReactContextBaseJavaModule(context) {
  private val executor: ExecutorService = Executors.newSingleThreadExecutor()
  private val connectionMonitor: ExecutorService = Executors.newSingleThreadExecutor()
  private val monitorGeneration = AtomicLong()
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
          startStatusMonitor()
        } else if (!client.isConnected()) {
          releaseMulticastLock()
          context.stopService(CosyncForegroundService.intent(context))
        }
        promise.resolve(result)
        emitStatus(client.isConnected())
      } catch (error: Exception) {
        if (!client.isConnected()) {
          releaseMulticastLock()
          context.stopService(CosyncForegroundService.intent(context))
          emitStatus(false)
        }
        promise.reject("PAIRING_FAILED", error.message, error)
      }
    }
  }

  @ReactMethod
  fun isConnected(promise: Promise) {
    executor.execute { promise.resolve(client.isConnected()) }
  }

  @ReactMethod
  fun recentDiagnostics(promise: Promise) {
    executor.execute { promise.resolve(client.recentDiagnostics()) }
  }

  @ReactMethod
  fun disconnect(promise: Promise) {
    executor.execute {
      monitorGeneration.incrementAndGet()
      client.disconnect()
      releaseMulticastLock()
      context.stopService(CosyncForegroundService.intent(context))
      emitStatus(false)
      promise.resolve(null)
    }
  }

  override fun invalidate() {
    monitorGeneration.incrementAndGet()
    client.disconnect()
    releaseMulticastLock()
    context.stopService(CosyncForegroundService.intent(context))
    connectionMonitor.shutdownNow()
    executor.shutdownNow()
    super.invalidate()
  }

  private fun startStatusMonitor() {
    val generation = monitorGeneration.incrementAndGet()
    try {
      connectionMonitor.execute {
        client.waitForDisconnect()
        try {
          executor.execute disconnectEvent@{
            if (monitorGeneration.get() != generation || client.isConnected()) {
              return@disconnectEvent
            }
            releaseMulticastLock()
            context.stopService(CosyncForegroundService.intent(context))
            emitStatus(false)
          }
        } catch (_: RejectedExecutionException) {
          // React Native is already tearing down this module.
        }
      }
    } catch (_: RejectedExecutionException) {
      // React Native is already tearing down this module.
    }
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
