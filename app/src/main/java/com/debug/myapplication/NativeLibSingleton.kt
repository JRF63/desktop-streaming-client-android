package com.debug.myapplication

import android.view.Surface

object NativeLibSingleton {
    private var nativeInstance: Long = 0
    private var mediaPlayerActivity: MediaPlayerActivity? = null

    @JvmName("createNativeInstance")
    private external fun createNativeInstance(nativeInstance: Long): Long
    @JvmName("destroyNativeInstance")
    private external fun destroyNativeInstance(nativeInstance: Long)

    init {
        System.loadLibrary("client_android")
        nativeInstance = createNativeInstance(0)
    }

    fun destroy() {
        if (nativeInstance != 0L) {
            destroyNativeInstance(nativeInstance)
        }
    }

    fun mediaPlayerCreated(mediaPlayer: MediaPlayerActivity) {
        mediaPlayerActivity = mediaPlayer
        // TODO
    }

    fun mediaPlayerDestroyed() {
        mediaPlayerActivity = null
    }

    fun mediaPlayerSurfaceCreated(surface: Surface) {
        // TODO
    }

    fun mediaPlayerSurfaceDestroyed() {
        // TODO
    }

    // Called by native code
    fun setMediaPlayerAspectRatio(width: Int, height: Int) {
        mediaPlayerActivity?.setSurfaceViewAspectRatio(width, height)
    }
}