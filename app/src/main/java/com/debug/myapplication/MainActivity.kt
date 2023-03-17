package com.debug.myapplication

import android.content.Intent
import android.media.MediaCodecList
import android.os.Bundle
import android.util.Log
import android.view.View
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.app.AppCompatDelegate
import androidx.appcompat.app.AppCompatDelegate.MODE_NIGHT_YES
import androidx.preference.PreferenceManager
import com.debug.myapplication.databinding.ActivityMainBinding

const val FIRST_RUN = "first_run"

class MainActivity : AppCompatActivity() {
    private lateinit var binding: ActivityMainBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        val sharedPreferences = PreferenceManager.getDefaultSharedPreferences(this)
        val firstRun = sharedPreferences.getBoolean(FIRST_RUN, true)
        if (firstRun) {
            sharedPreferences.edit().putBoolean(FIRST_RUN, false).apply()
            // TODO: Init settings
        }

        supportFragmentManager
            .beginTransaction()
            .replace(binding.root.id, MainSettingsFragment())
            .commit()

        AppCompatDelegate.setDefaultNightMode(MODE_NIGHT_YES)
    }

    override fun onDestroy() {
        super.onDestroy()
        NativeLibSingleton.destroy()
    }

//    fun startStreaming(view: View) {
////        val intent = Intent(this, MediaPlayerActivity::class.java)
////        startActivity(intent)
//
//        supportFragmentManager
//            .beginTransaction()
//            .replace(binding.root.id, MainSettingsFragment())
//            .commit()
//
//    }
}