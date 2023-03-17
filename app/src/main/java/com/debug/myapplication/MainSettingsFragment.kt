package com.debug.myapplication

import android.os.Bundle
import androidx.preference.PreferenceFragmentCompat

class MainSettingsFragment : PreferenceFragmentCompat() {
    override fun onCreatePreferences(savedInstanceState: Bundle?, rootKey: String?) {
        setPreferencesFromResource(R.xml.preferences, rootKey)
    }
}
