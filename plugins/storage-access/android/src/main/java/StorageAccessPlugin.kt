package dev.bhandara.storageaccess

import android.Manifest
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

// Android 11+ grants shared-storage-wide reads only through the
// "All files access" settings screen; older versions use READ_EXTERNAL_STORAGE.
@TauriPlugin
class StorageAccessPlugin(private val activity: Activity) : Plugin(activity) {
    private fun granted(): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Environment.isExternalStorageManager()
        } else {
            ContextCompat.checkSelfPermission(activity, Manifest.permission.READ_EXTERNAL_STORAGE) ==
                PackageManager.PERMISSION_GRANTED
        }

    @Command
    fun status(invoke: Invoke) {
        val result = JSObject()
        result.put("granted", granted())
        invoke.resolve(result)
    }

    @Command
    fun request(invoke: Invoke) {
        if (!granted()) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val appScreen = Intent(
                    Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:" + activity.packageName),
                )
                try {
                    activity.startActivity(appScreen)
                } catch (e: ActivityNotFoundException) {
                    activity.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
                }
            } else {
                ActivityCompat.requestPermissions(activity, arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE), 0)
            }
        }
        invoke.resolve()
    }
}
