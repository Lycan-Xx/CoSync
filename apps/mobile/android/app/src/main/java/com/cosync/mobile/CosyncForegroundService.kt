package com.cosync.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.IBinder
import androidx.core.app.NotificationCompat

/** Owns the process lifetime for the persistent trusted Cosync session. */
class CosyncForegroundService : Service() {
  companion object {
    const val ACTION_STOP = "com.cosync.mobile.STOP_CONNECTION"
    private const val CHANNEL_ID = "cosync_connection"
    private const val NOTIFICATION_ID = 57823

    fun intent(context: android.content.Context): Intent =
      Intent(context, CosyncForegroundService::class.java)
  }

  private lateinit var connectionManager: CosyncConnectionManager
  private val statusListener: (Boolean) -> Unit = { connected ->
    getSystemService(NotificationManager::class.java)
      .notify(NOTIFICATION_ID, buildNotification(connected))
  }

  override fun onCreate() {
    super.onCreate()
    val notificationManager = getSystemService(NotificationManager::class.java)
    notificationManager.createNotificationChannel(
      NotificationChannel(CHANNEL_ID, "Cosync connection", NotificationManager.IMPORTANCE_LOW)
    )
    connectionManager = CosyncConnectionManager.get(this)
    connectionManager.addListener(statusListener)
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      connectionManager.stop()
      stopForeground(STOP_FOREGROUND_REMOVE)
      stopSelf()
      return START_NOT_STICKY
    }

    startForeground(NOTIFICATION_ID, buildNotification(connectionManager.isConnected()))
    connectionManager.start()
    return START_STICKY
  }

  override fun onDestroy() {
    connectionManager.removeListener(statusListener)
    super.onDestroy()
  }

  override fun onBind(intent: Intent?): IBinder? = null

  private fun buildNotification(connected: Boolean): Notification {
    val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
    val pendingIntent = launchIntent?.let {
      PendingIntent.getActivity(
        this,
        0,
        it,
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
      )
    }
    return NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.stat_sys_upload)
      .setContentTitle(if (connected) "Cosync connected" else "Cosync reconnecting")
      .setContentText(
        if (connected) "Your paired desktop is available" else "Waiting for your paired desktop"
      )
      .setOngoing(true)
      .apply { if (pendingIntent != null) setContentIntent(pendingIntent) }
      .build()
  }
}
