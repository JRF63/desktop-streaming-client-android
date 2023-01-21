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

//        val mimeType = "video/avc"
//        val decoder = NativeLibSingleton.chooseDecoderForType(mimeType)!!
//        Log.i("client-android", decoder)
//        val profiles = NativeLibSingleton.listProfilesForDecoder(decoder, mimeType)
//        for (p in profiles!!) {
//            Log.i("client-android", " $p")
//        }
    }
}