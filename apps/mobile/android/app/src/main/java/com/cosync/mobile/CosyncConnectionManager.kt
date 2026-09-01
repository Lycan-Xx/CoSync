package com.cosync.mobile

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.WifiManager
import androidx.core.content.ContextCompat
import java.util.concurrent.CopyOnWriteArraySet
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong
import uniffi.cosync_mobile.ConnectionClient

/**
 * Process-wide owner of the Rust connection client.
 *
 * React activities and bridges can be destroyed and recreated while this
 * object remains owned by the foreground-service process. Network callbacks
 * wake bounded reconnect attempts; the app never polls connection status.
 */
class CosyncConnectionManager private constructor(context: Context) {
  companion object {
    @Volatile private var instance: CosyncConnectionManager? = null

    fun get(context: Context): CosyncConnectionManager =
      instance ?: synchronized(this) {
        instance ?: CosyncConnectionManager(context.applicationContext).also { instance = it }
      }
  }

  private val appContext = context.applicationContext
  private val client = ConnectionClient(appContext.filesDir.resolve("cosync").absolutePath)
  private val connectionExecutor = Executors.newSingleThreadExecutor()
  private val monitorExecutor = Executors.newSingleThreadExecutor()
  private val retryScheduler = Executors.newSingleThreadScheduledExecutor()
  private val listeners = CopyOnWriteArraySet<(Boolean) -> Unit>()
  private val monitorGeneration = AtomicLong()
  private val connectivity = appContext.getSystemService(ConnectivityManager::class.java)
  private val wifi = appContext.getSystemService(WifiManager::class.java)

  @Volatile private var active = false
  @Volatile private var connected = client.isConnected()
  @Volatile private var networkCallbackRegistered = false
  private var retryAttempt = 0
  private var scheduledRetry: ScheduledFuture<*>? = null
  private var multicastLock: WifiManager.MulticastLock? = null

  private val networkCallback = object : ConnectivityManager.NetworkCallback() {
    override fun onAvailable(network: Network) {
      if (active) scheduleReconnect(0)
    }

    override fun onLost(network: Network) {
      if (!active) return
      connectionExecutor.execute {
        if (!hasWifiNetwork()) {
          monitorGeneration.incrementAndGet()
          client.disconnect()
          cancelScheduledRetry()
          setConnected(false)
        }
      }
    }
  }

  fun addListener(listener: (Boolean) -> Unit) {
    listeners.add(listener)
    listener(connected)
  }

  fun removeListener(listener: (Boolean) -> Unit) {
    listeners.remove(listener)
  }

  fun isConnected(): Boolean = connected && client.isConnected()

  fun resumeIfPaired() {
    connectionExecutor.execute {
      if (client.hasPairedDevice()) {
        ContextCompat.startForegroundService(appContext, CosyncForegroundService.intent(appContext))
      }
    }
  }

  fun start() {
    if (active) return
    active = true
    registerNetworkCallback()
    connectionExecutor.execute {
      if (client.isConnected()) {
        retryAttempt = 0
        setConnected(true)
        startDisconnectMonitor()
      } else {
        setConnected(false)
        scheduleReconnect(0)
      }
    }
  }

  fun pair(payload: String, deviceName: String, callback: (Result<String>) -> Unit) {
    connectionExecutor.execute {
      try {
        val result = client.pair(payload, deviceName)
        if (result == "connected") {
          active = true
          registerNetworkCallback()
          retryAttempt = 0
          cancelScheduledRetry()
          setConnected(true)
          startDisconnectMonitor()
          ContextCompat.startForegroundService(
            appContext,
            CosyncForegroundService.intent(appContext)
          )
        } else {
          // A rejected replacement QR must not disturb the current session.
          setConnected(client.isConnected())
        }
        callback(Result.success(result))
      } catch (error: Exception) {
        setConnected(client.isConnected())
        callback(Result.failure(error))
      }
    }
  }

  fun recentDiagnostics(callback: (Result<String>) -> Unit) {
    connectionExecutor.execute {
      try {
        callback(Result.success(client.recentDiagnostics()))
      } catch (error: Exception) {
        callback(Result.failure(error))
      }
    }
  }

  fun stop(callback: (() -> Unit)? = null) {
    active = false
    unregisterNetworkCallback()
    cancelScheduledRetry()
    monitorGeneration.incrementAndGet()
    connectionExecutor.execute {
      client.disconnect()
      releaseMulticastLock()
      setConnected(false)
      callback?.invoke()
    }
  }

  private fun scheduleReconnect(delayMs: Long) {
    if (!active || connected) return
    synchronized(this) {
      scheduledRetry?.cancel(false)
      scheduledRetry = retryScheduler.schedule({
        try {
          connectionExecutor.execute { attemptReconnect() }
        } catch (_: RejectedExecutionException) {
          // The process is shutting down.
        }
      }, delayMs, TimeUnit.MILLISECONDS)
    }
  }

  private fun attemptReconnect() {
    if (!active || connected || !hasWifiNetwork()) return
    acquireMulticastLock()
    val result = try {
      client.reconnect()
    } finally {
      releaseMulticastLock()
    }

    if (result == "connected") {
      retryAttempt = 0
      setConnected(true)
      startDisconnectMonitor()
      return
    }

    setConnected(false)
    val delayMs = (500L shl retryAttempt.coerceAtMost(10)).coerceAtMost(60_000L)
    retryAttempt = (retryAttempt + 1).coerceAtMost(10)
    scheduleReconnect(delayMs)
  }

  private fun startDisconnectMonitor() {
    val generation = monitorGeneration.incrementAndGet()
    try {
      monitorExecutor.execute {
        client.waitForDisconnect()
        try {
          connectionExecutor.execute disconnectEvent@{
            if (monitorGeneration.get() != generation || client.isConnected()) {
              return@disconnectEvent
            }
            setConnected(false)
            if (active && hasWifiNetwork()) scheduleReconnect(0)
          }
        } catch (_: RejectedExecutionException) {
          // The process is shutting down.
        }
      }
    } catch (_: RejectedExecutionException) {
      // The process is shutting down.
    }
  }

  private fun setConnected(value: Boolean) {
    if (connected == value) return
    connected = value
    listeners.forEach { listener -> listener(value) }
  }

  private fun hasWifiNetwork(): Boolean {
    return connectivity.allNetworks.any { network ->
      connectivity.getNetworkCapabilities(network)
        ?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true
    }
  }

  private fun registerNetworkCallback() {
    synchronized(this) {
      if (networkCallbackRegistered) return
      val request = NetworkRequest.Builder()
        .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
        .build()
      connectivity.registerNetworkCallback(request, networkCallback)
      networkCallbackRegistered = true
    }
  }

  private fun unregisterNetworkCallback() {
    synchronized(this) {
      if (!networkCallbackRegistered) return
      try {
        connectivity.unregisterNetworkCallback(networkCallback)
      } catch (_: IllegalArgumentException) {
        // Already unregistered by Android during process teardown.
      }
      networkCallbackRegistered = false
    }
  }

  private fun cancelScheduledRetry() {
    synchronized(this) {
      scheduledRetry?.cancel(false)
      scheduledRetry = null
    }
  }

  private fun acquireMulticastLock() {
    if (multicastLock?.isHeld == true) return
    multicastLock = wifi.createMulticastLock("cosync-discovery").apply {
      setReferenceCounted(false)
      acquire()
    }
  }

  private fun releaseMulticastLock() {
    multicastLock?.let { if (it.isHeld) it.release() }
    multicastLock = null
  }
}
