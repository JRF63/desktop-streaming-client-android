package com.debug.myapplication

import android.os.Bundle
import android.view.Surface
import android.view.SurfaceHolder
import androidx.appcompat.app.AppCompatActivity
import androidx.constraintlayout.widget.ConstraintSet
import com.debug.myapplication.databinding.ActivityStreamingBinding

class StreamingActivity : AppCompatActivity() {

    private lateinit var binding: ActivityStreamingBinding
    private val layoutConstraints: ConstraintSet = ConstraintSet()
    private var nativeInstance: ULong = 0u
    // TODO: Create a dummy SurfaceTexture for switching when the SurfaceView's texture is destroyed

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        binding = ActivityStreamingBinding.inflate(layoutInflater)
        setContentView(binding.root)
        layoutConstraints.clone(binding.root)

        nativeInstance = createNativeInstance(nativeInstance)
        // TODO: If zero, switch back to main activity

        binding.surfaceView.holder.addCallback(object: SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                sendSurfaceCreated(nativeInstance, holder.surface)
            }

            override fun surfaceChanged(p0: SurfaceHolder, p1: Int, p2: Int, p3: Int) {}

            override fun surfaceDestroyed(p0: SurfaceHolder) {
                sendSurfaceDestroyed(nativeInstance)
            }
        })
    }

    override fun onDestroy() {
        super.onDestroy()
        sendDestroySignal(nativeInstance)
    }

    // Called by native code
    fun setSurfaceViewAspectRatio(aspectRatio: String) {
        this@StreamingActivity.runOnUiThread {
            layoutConstraints.setDimensionRatio(binding.surfaceView.id, aspectRatio)
            layoutConstraints.applyTo(binding.root)
        }
    }

    // Single letter function names for obfuscation and easier interfacing in
    // native code - this prevents appending random characters to the function
    // signatures (i.e., createNativeInstance => createNativeInstance-V0uzKk8)

    @JvmName("a")
    private external fun createNativeInstance(nativeInstance: ULong): ULong
    @JvmName("b")
    private external fun sendDestroySignal(nativeInstance: ULong)
    @JvmName("c")
    private external fun sendSurfaceCreated(nativeInstance: ULong, surface: Surface)
    @JvmName("d")
    private external fun sendSurfaceDestroyed(nativeInstance: ULong)

    companion object {
        init {
            System.loadLibrary("client_android")
        }
    }
}