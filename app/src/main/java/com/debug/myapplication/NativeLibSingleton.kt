package com.debug.myapplication

import android.view.Surface

object NativeLibSingleton {
    private var nativeInstance: Long = 0
    private var mediaPlayerActivity: MediaPlayerActivity? = null

    @JvmName("createNativeInstance")
    private external fun createNativeInstance(): Long
    @JvmName("destroyNativeInstance")
    private external fun destroyNativeInstance(nativeInstance: Long)

    @JvmName("sendSurface")
    private external fun sendSurface(nativeInstance: Long, surface: Surface)
    @JvmName("destroySurface")
    private external fun destroySurface(nativeInstance: Long)

    @JvmName("startMediaPlayer")
    private external fun startMediaPlayer(nativeInstance: Long)

    init {
        System.loadLibrary("client_android")
        nativeInstance = createNativeInstance()
    }

    fun destroy() {
        if (nativeInstance != 0L) {
            destroyNativeInstance(nativeInstance)
        }
    }

    fun mediaPlayerCreated(mediaPlayer: MediaPlayerActivity) {
        mediaPlayerActivity = mediaPlayer
        startMediaPlayer(nativeInstance)
    }

    fun mediaPlayerDestroyed() {
        mediaPlayerActivity = null
    }

    fun mediaPlayerSurfaceCreated(surface: Surface) {
        sendSurface(nativeInstance, surface)
    }

    fun mediaPlayerSurfaceDestroyed() {
        destroySurface(nativeInstance)
    }

    // Called by native code
    private fun setMediaPlayerAspectRatio(width: Int, height: Int) {
        mediaPlayerActivity?.setSurfaceViewAspectRatio(width, height)
    }

    fun chooseDecoderForType(mimeType: String): String? {
        return CodecQuerySingleton.chooseDecoderForType(mimeType)
    }

    fun listProfilesForDecoder(decoderName: String, mimeType: String): IntArray? {
        return CodecQuerySingleton.listProfilesForDecoder(decoderName, mimeType)?.toIntArray()
    }
}