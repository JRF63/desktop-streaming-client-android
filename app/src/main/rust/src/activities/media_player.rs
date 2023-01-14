use jni::{objects::GlobalRef, JNIEnv, JavaVM};

/// Struct representing the host Android activity.
pub struct MediaPlayerActivity {
    vm: JavaVM,
    activity: GlobalRef,
}

impl MediaPlayerActivity {
    pub fn new(
        env: &JNIEnv,
        activity: jni::sys::jobject,
    ) -> Result<MediaPlayerActivity, jni::errors::Error> {
        Ok(Self {
            vm: env.get_java_vm()?,
            activity: env.new_global_ref(activity)?,
        })
    }

    pub fn set_surface_view_aspect_ratio(
        &self,
        env: &JNIEnv,
        width: i32,
        height: i32,
    ) -> Result<(), jni::errors::Error> {
        let (width, height) = ratio_to_lowest_terms(width, height);
        let obj = self.activity.as_obj();
        env.call_method(
            obj,
            "setSurfaceViewAspectRatio",
            "(II)V",
            &[width.into(), height.into()],
        )?;
        Ok(())
    }
}

fn ratio_to_lowest_terms(width: i32, height: i32) -> (i32, i32) {
    //https://en.wikipedia.org/wiki/Binary_GCD_algorithm
    pub fn gcd(mut u: i32, mut v: i32) -> i32 {
        use std::cmp::min;
        use std::mem::swap;

        if u == 0 {
            return v;
        } else if v == 0 {
            return u;
        }

        let i = u.trailing_zeros();
        u >>= i;
        let j = v.trailing_zeros();
        v >>= j;
        let k = min(i, j);

        loop {
            if u > v {
                swap(&mut u, &mut v);
            }
            v -= u;
            if v == 0 {
                return u << k;
            }
            v >>= v.trailing_zeros();
        }
    }
    let divisor = gcd(width, height);
    (width / divisor, height / divisor)
}