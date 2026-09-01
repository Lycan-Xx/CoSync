package com.cosync.mobile

import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.modules.core.DeviceEventManagerModule

class CosyncBridgeModule(private val context: ReactApplicationContext) :
  ReactContextBaseJavaModule(context) {
  private val manager = CosyncConnectionManager.get(context)
  private val statusListener: (Boolean) -> Unit = { connected -> emitStatus(connected) }

  init {
    manager.addListener(statusListener)
    manager.resumeIfPaired()
  }

  override fun getName(): String = "Cosync"

  @ReactMethod
  fun pair(payload: String, deviceName: String, promise: Promise) {
    manager.pair(payload, deviceName) { result ->
      result.fold(
        onSuccess = promise::resolve,
        onFailure = { error -> promise.reject("PAIRING_FAILED", error.message, error) }
      )
    }
  }

  @ReactMethod
  fun isConnected(promise: Promise) {
    promise.resolve(manager.isConnected())
  }

  @ReactMethod
  fun recentDiagnostics(promise: Promise) {
    manager.recentDiagnostics { result ->
      result.fold(
        onSuccess = promise::resolve,
        onFailure = { error -> promise.reject("DIAGNOSTICS_FAILED", error.message, error) }
      )
    }
  }

  @ReactMethod
  fun disconnect(promise: Promise) {
    manager.stop {
      context.stopService(CosyncForegroundService.intent(context))
      promise.resolve(null)
    }
  }

  override fun invalidate() {
    // React bridge/activity teardown must not own or close the connection.
    manager.removeListener(statusListener)
    super.invalidate()
  }

  private fun emitStatus(connected: Boolean) {
    if (!context.hasActiveReactInstance()) return
    context
      .getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java)
      .emit("cosyncConnectionState", connected)
  }
}
