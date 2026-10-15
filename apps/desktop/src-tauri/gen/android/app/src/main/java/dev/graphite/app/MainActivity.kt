package dev.graphite.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.enableEdgeToEdge
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * The share sheet target. Anything shared into Graphite (text, a link,
 * images, files) is written to `files/shared/pending.json` with the files
 * copied next to it; the web side picks the queue up through the
 * `take_shared` command when it comes to the front and files each entry
 * as a capture in the vault's inbox.
 */
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    intent?.let { queueShare(it) }
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    queueShare(intent)
  }

  private fun queueShare(intent: Intent) {
    val action = intent.action
    if (action != Intent.ACTION_SEND && action != Intent.ACTION_SEND_MULTIPLE) return
    val entry = JSONObject()
    entry.put("text", intent.getStringExtra(Intent.EXTRA_TEXT) ?: "")
    entry.put("subject", intent.getStringExtra(Intent.EXTRA_SUBJECT) ?: "")
    val files = JSONArray()
    val uris = ArrayList<Uri>()
    if (action == Intent.ACTION_SEND) {
      @Suppress("DEPRECATION")
      (intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM))?.let { uris.add(it) }
    } else {
      @Suppress("DEPRECATION")
      intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)?.let { uris.addAll(it) }
    }
    val dir = File(filesDir, "shared")
    dir.mkdirs()
    for (uri in uris) {
      try {
        val name = displayName(uri)
        val target = File(dir, "${System.currentTimeMillis()}-$name")
        contentResolver.openInputStream(uri)?.use { input ->
          target.outputStream().use { output -> input.copyTo(output) }
        }
        files.put(target.absolutePath)
      } catch (_: Exception) {
        // A stream that cannot be read is skipped; the text still arrives.
      }
    }
    entry.put("files", files)
    // Consume the intent so a rotation does not queue it again.
    intent.action = Intent.ACTION_MAIN
    intent.removeExtra(Intent.EXTRA_TEXT)
    intent.removeExtra(Intent.EXTRA_STREAM)
    val pending = File(dir, "pending.json")
    val queue = try {
      if (pending.exists()) JSONArray(pending.readText()) else JSONArray()
    } catch (_: Exception) {
      JSONArray()
    }
    queue.put(entry)
    pending.writeText(queue.toString())
  }

  private fun displayName(uri: Uri): String {
    var name: String? = null
    try {
      contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
        if (c.moveToFirst()) name = c.getString(0)
      }
    } catch (_: Exception) {}
    val raw = name ?: uri.lastPathSegment ?: "shared"
    return raw.replace(Regex("[^A-Za-z0-9._ -]"), "-").ifBlank { "shared" }
  }
}
