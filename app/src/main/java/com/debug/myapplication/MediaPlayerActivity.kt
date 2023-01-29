package com.debug.myapplication

import android.os.Bundle
import android.view.SurfaceHolder
import androidx.appcompat.app.AppCompatActivity
import androidx.constraintlayout.widget.ConstraintSet
import com.debug.myapplication.databinding.ActivityStreamingBinding

class MediaPlayerActivity : AppCompatActivity() {

    private lateinit var binding: ActivityStreamingBinding
    private val layoutConstraints: ConstraintSet = ConstraintSet()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        binding = ActivityStreamingBinding.inflate(layoutInflater)
        setContentView(binding.root)
        layoutConstraints.clone(binding.root)

        binding.surfaceView.keepScreenOn = true;

        binding.surfaceView.holder.addCallback(object: SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                NativeLibSingleton.mediaPlayerSurfaceCreated(holder.surface)
            }

            override fun surfaceChanged(p0: SurfaceHolder, p1: Int, p2: Int, p3: Int) {}

            override fun surfaceDestroyed(p0: SurfaceHolder) {
                NativeLibSingleton.mediaPlayerSurfaceDestroyed()
            }
        })

        NativeLibSingleton.mediaPlayerCreated(this)
    }

    override fun onDestroy() {
        super.onDestroy()
        NativeLibSingleton.mediaPlayerDestroyed()
    }

    fun setSurfaceViewAspectRatio(width: Int, height: Int) {
        this@MediaPlayerActivity.runOnUiThread {
            layoutConstraints.setDimensionRatio(binding.surfaceView.id, "$width:$height")
            layoutConstraints.applyTo(binding.root)
        }
    }
}