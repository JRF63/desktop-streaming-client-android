use jni::{objects::GlobalRef, JNIEnv, JavaVM};

/// Struct representing the host Android activity.
pub struct AndroidActivity {
    vm: JavaVM,
    activity: GlobalRef,
}

impl AndroidActivity {
    pub fn new(
        env: &JNIEnv,
        activity: jni::sys::jobject,
    ) -> Result<AndroidActivity, jni::errors::Error> {
        Ok(Self {
            vm: env.get_java_vm()?,
            activity: env.new_global_ref(activity)?,
        })
    }

    pub fn set_surface_view_aspect_ratio(
        &self,
        env: &JNIEnv,
        aspect_ratio_string: String,
    ) -> Result<(), jni::errors::Error> {
        let aspect_ratio_string = env.new_string(aspect_ratio_string)?;
        let obj = self.activity.as_obj();
        env.call_method(
            obj,
            "setSurfaceViewAspectRatio",
            "(Ljava/lang/String;)V",
            &[aspect_ratio_string.into()],
        )?;
        Ok(())
    }
}
