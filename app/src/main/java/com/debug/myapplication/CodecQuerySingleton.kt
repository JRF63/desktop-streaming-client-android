package com.debug.myapplication

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecInfo.CodecCapabilities.FEATURE_LowLatency
import android.media.MediaCodecInfo.CodecProfileLevel.*
import android.media.MediaCodecList
import android.os.Build
import android.util.Log

object CodecQuerySingleton {
    private val softwareDecoderPattern = "OMX.(google|SEC)".toRegex()

    // TODO: Expose this setting
    private var preferBaselineProfile: Boolean = false
    private val h264ProfilePreference: MutableMap<Int, Int> by lazy { initH264ProfilePreference() }
    private val hevcProfilePreference: MutableMap<Int, Int> by lazy { initHevcProfilePreference() }
    private val av1ProfilePreference: MutableMap<Int, Int> by lazy { initAv1ProfilePreference() }

    private fun initH264ProfilePreference(): MutableMap<Int, Int> {
        val h264ProfilePreference: MutableMap<Int, Int> = mutableMapOf(
            AVCProfileMain to 11,
            AVCProfileExtended to 12,
            AVCProfileHigh10 to 13,
            AVCProfileHigh422 to 14,
            AVCProfileHigh444 to 15,
        )
        if (preferBaselineProfile) {
            h264ProfilePreference[AVCProfileBaseline] = 0
            h264ProfilePreference[AVCProfileHigh] = 10
        } else {
            h264ProfilePreference[AVCProfileBaseline] = 10
            h264ProfilePreference[AVCProfileHigh] = 0
        }
        // Prefer these over their non-constrained counterparts
        if (Build.VERSION.SDK_INT >= 27) {
            h264ProfilePreference[AVCProfileConstrainedBaseline] =
                h264ProfilePreference[AVCProfileBaseline]!! - 1
            h264ProfilePreference[AVCProfileConstrainedHigh] =
                h264ProfilePreference[AVCProfileHigh]!! - 1
        }

        return h264ProfilePreference
    }

    private fun initHevcProfilePreference(): MutableMap<Int, Int> {
        return mutableMapOf(
            HEVCProfileMain to 0,
            HEVCProfileMain10 to 1,
        )
    }

    private fun initAv1ProfilePreference(): MutableMap<Int, Int> {
        val av1ProfilePreference: MutableMap<Int, Int> = mutableMapOf()
        if (Build.VERSION.SDK_INT >= 29) {
            av1ProfilePreference[AV1ProfileMain8] = 0
            av1ProfilePreference[AV1ProfileMain10] = 1
            av1ProfilePreference[AV1ProfileMain10HDR10] = 2
            av1ProfilePreference[AV1ProfileMain10HDR10Plus] = 3
        }

        return av1ProfilePreference
    }

    private fun listDecodersForType(mimeType: String): List<MediaCodecInfo> {
        return MediaCodecList(MediaCodecList.ALL_CODECS).codecInfos.filter {
                // Decoders only
                !it.isEncoder
            }.filter {
                // Include only those that support `mimeType`
                it.supportedTypes.any { type -> type.equals(mimeType, ignoreCase = true) }
            }
    }

    fun chooseDecoderForType(mimeType: String): String? {
        val preference: MutableMap<Int, Int> = when (mimeType) {
            "video/av01" -> av1ProfilePreference
            "video/hevc" -> hevcProfilePreference
            "video/avc" -> h264ProfilePreference
            else -> null
        } ?: return null

        val entries = mutableListOf<DecoderEntry>()

        for (decoderInfo in listDecodersForType(mimeType)) {
            val capabilities = decoderInfo.getCapabilitiesForType(mimeType)

            val isHardwareAccelerated = if (Build.VERSION.SDK_INT >= 29) {
                decoderInfo.isHardwareAccelerated
            } else {
                !softwareDecoderPattern.containsMatchIn(decoderInfo.name)
            }
            val isLowLatency = Build.VERSION.SDK_INT >= 30 && capabilities.isFeatureSupported(
                FEATURE_LowLatency
            )

            for (profileLevel in capabilities.profileLevels) {
                entries.add(
                    DecoderEntry(
                        decoderInfo.name, isHardwareAccelerated, isLowLatency, profileLevel.profile
                    )
                )
            }
        }

        // Prefer decoders with low latency and is hardware accel., and sort by profile pref.
        entries.sortWith(compareBy({ !it.isLowLatency },
            { !it.isHardwareAccelerated },
            { preference[it.profile] ?: Int.MAX_VALUE }))

        return entries.firstOrNull()?.name
    }

    fun listProfilesForDecoder(decoderName: String, mimeType: String): List<Int>? {
        return try {
            val decoder = MediaCodec.createByCodecName(decoderName)
            decoder.codecInfo.getCapabilitiesForType(mimeType).profileLevels?.map { it.profile }
        } catch (e: java.lang.Exception) {
            null
        }
    }

    fun listDecoders() {
        val mimeTypes = listOf("video/av01", "video/hevc", "video/avc")

        for (mimeType in mimeTypes) {
            val decoderName = chooseDecoderForType(mimeType)
            Log.i("client-android", "$mimeType $decoderName")
        }
    }
}

data class DecoderEntry(
    val name: String,
    val isHardwareAccelerated: Boolean,
    val isLowLatency: Boolean,
    val profile: Int
)