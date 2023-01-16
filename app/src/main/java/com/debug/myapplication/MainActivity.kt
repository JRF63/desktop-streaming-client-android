package com.debug.myapplication

import android.content.Intent
import android.media.MediaCodecList
import android.os.Bundle
import android.util.Log
import android.view.View
import androidx.appcompat.app.AppCompatActivity
import com.debug.myapplication.databinding.ActivityMainBinding

class MainActivity : AppCompatActivity() {
    private lateinit var binding: ActivityMainBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)
    }

    override fun onDestroy() {
        super.onDestroy()
        NativeLibSingleton.destroy()
    }

    fun startStreaming(view: View) {
        val intent = Intent(this, MediaPlayerActivity::class.java)
        startActivity(intent)

//        listDecoders("video/avc")
//        listDecoders("video/hevc")
    }

//    private fun listDecoders(mimeType: String) {
//        val decoders =
//            MediaCodecList(MediaCodecList.ALL_CODECS).codecInfos.filter { !it.isEncoder }.filter {
//                it.supportedTypes.any { type -> type.equals(mimeType, ignoreCase = true) }
//            }
//        Log.i("client-android", mimeType)
//        for (codecInfo in decoders) {
//            Log.i("client-android", "  ${codecInfo.name}")
//            val capabilities = codecInfo.getCapabilitiesForType(mimeType)
//            for (profile in capabilities.profileLevels) {
//                Log.i("client-android", "    ${profile.profile}, ${profile.level}")
//            }
//        }
//    }
}