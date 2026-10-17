#[cfg(feature = "sdl")]
use jni::refs::IntoAuto;
use jni::{
    Env, JValue, JavaVM, jni_sig, jni_str,
    objects::{JClass, JObject},
    refs::Global,
};

use crate::ui::clipboard::{
    ClipboardData, ClipboardDispatch, ClipboardError, ClipboardRead, ClipboardRequest,
    ClipboardResponse, ClipboardWrite,
};

mod wire;
use wire::{Failure, decode, encode};

struct Host {
    vm: JavaVM,
    session: Global<JObject<'static>>,
    payload: Global<JClass<'static>>,
}

impl Host {
    #[cfg(feature = "sdl")]
    fn sdl() -> Result<Self, ClipboardError> {
        unsafe extern "C" {
            fn SDL_AndroidGetJNIEnv() -> *mut jni::sys::JNIEnv;
            fn SDL_AndroidGetActivity() -> jni::sys::jobject;
        }
        // SDL owns this thread attachment while its video subsystem is alive.
        let pointer = unsafe { SDL_AndroidGetJNIEnv() };
        if pointer.is_null() {
            return Err(ClipboardError::Unavailable);
        }
        let mut unowned = unsafe { jni::EnvUnowned::from_raw(pointer) };
        let vm = match unowned.with_env(|env| env.get_java_vm()).into_outcome() {
            jni::Outcome::Ok(vm) => vm,
            jni::Outcome::Err(_) => return Err(ClipboardError::Platform),
            jni::Outcome::Panic(panic) => std::panic::resume_unwind(panic),
        };
        vm.attach_current_thread(|env| {
            // SDL returns a fresh local reference; this frame owns and releases it.
            let activity = unsafe { JObject::from_raw(env, SDL_AndroidGetActivity()) }.auto();
            Self::new(env, &activity)
        })
        .map_err(Failure::clipboard)
    }

    fn new(env: &mut Env<'_>, context: &JObject<'_>) -> Result<Self, Failure> {
        let context = env
            .call_method(
                context,
                jni_str!("getApplicationContext"),
                jni_sig!("()Landroid/content/Context;"),
                &[],
            )?
            .l()?;
        let loader = env
            .call_method(
                &context,
                jni_str!("getClassLoader"),
                jni_sig!("()Ljava/lang/ClassLoader;"),
                &[],
            )?
            .l()?;
        let mut class = |name| -> Result<Global<JClass<'static>>, Failure> {
            let name = env.new_string(name)?;
            let class = env
                .call_method(
                    &loader,
                    jni_str!("loadClass"),
                    jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
                    &[JValue::Object(&name)],
                )?
                .l()?;
            Ok(env.new_cast_global_ref::<JClass>(class)?)
        };
        let session_type = class("rs.mirui.android.ClipboardSession")?;
        let payload = class("rs.mirui.android.ClipboardPayload")?;
        let session = env.new_object(
            &session_type,
            jni_sig!("(Landroid/content/Context;)V"),
            &[JValue::Object(&context)],
        )?;
        Ok(Self {
            vm: env.get_java_vm()?,
            session: env.new_global_ref(session)?,
            payload,
        })
    }

    #[cfg(feature = "wgpu")]
    fn winit() -> Result<Self, ClipboardError> {
        let context = ndk_context::android_context();
        // The active winit Android loop retains this native Context during initialization.
        let vm = unsafe { JavaVM::from_raw(context.vm().cast()) };
        vm.attach_current_thread(|env| {
            let raw = context.context().cast();
            // This is AndroidActivity's borrowed global reference, not owned by this bridge.
            let context = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
            Self::new(env, &context)
        })
        .map_err(Failure::clipboard)
    }

    fn submit(&self, request: &ClipboardRequest) -> Result<(), ClipboardError> {
        self.vm
            .attach_current_thread(|env| -> Result<(), Failure> {
                match request {
                    ClipboardRequest::Write(request) => {
                        let payload = encode(env, &self.payload, request.data())?;
                        env.call_method(
                            &self.session,
                            jni_str!("write"),
                            jni_sig!("(Lrs/mirui/android/ClipboardPayload;)V"),
                            &[JValue::Object(&payload)],
                        )?;
                    }
                    ClipboardRequest::Read(_) => {
                        env.call_method(&self.session, jni_str!("read"), jni_sig!("()V"), &[])?;
                    }
                    #[cfg(test)]
                    ClipboardRequest::DeferredRead(_, _) => {
                        return Err(ClipboardError::Unsupported.into());
                    }
                }
                Ok(())
            })
            .map_err(Failure::clipboard)
    }

    fn done(&self) -> Result<bool, ClipboardError> {
        self.vm
            .attach_current_thread(|env| -> Result<bool, Failure> {
                Ok(env
                    .call_method(&self.session, jni_str!("isDone"), jni_sig!("()Z"), &[])?
                    .z()?)
            })
            .map_err(Failure::clipboard)
    }

    fn take(&self, read: bool) -> Result<Option<ClipboardData>, ClipboardError> {
        self.vm
            .attach_current_thread(|env| -> Result<_, Failure> {
                let payload = env
                    .call_method(
                        &self.session,
                        jni_str!("take"),
                        jni_sig!("()Lrs/mirui/android/ClipboardPayload;"),
                        &[],
                    )?
                    .l()?;
                if read {
                    Ok(Some(decode(env, payload)?))
                } else if payload.is_null() {
                    Ok(None)
                } else {
                    Err(ClipboardError::InvalidData.into())
                }
            })
            .map_err(Failure::clipboard)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let result = self
            .vm
            .attach_current_thread(|env| -> jni::errors::Result<()> {
                env.call_method(&self.session, jni_str!("close"), jni_sig!("()V"), &[])?;
                Ok(())
            });
        if result.is_err() {
            crate::warn!(target: "mirui::clipboard", "Android clipboard cancellation failed");
        }
    }
}

#[derive(Default)]
pub(crate) struct Clipboard {
    host: Option<Host>,
    pending: Option<ClipboardRequest>,
    completed: Option<ClipboardResponse>,
}

impl Clipboard {
    #[cfg(feature = "sdl")]
    pub(crate) fn observe_sdl_event(&mut self, event: &sdl2::event::Event) {
        if matches!(
            event,
            sdl2::event::Event::AppWillEnterBackground { .. }
                | sdl2::event::Event::AppDidEnterBackground { .. }
                | sdl2::event::Event::AppTerminating { .. }
                | sdl2::event::Event::Quit { .. }
        ) {
            self.detach();
        }
    }

    #[cfg(feature = "sdl")]
    pub(crate) fn write_sdl(&mut self, request: ClipboardWrite) -> ClipboardDispatch {
        self.submit(ClipboardRequest::Write(request), Host::sdl)
    }

    #[cfg(feature = "sdl")]
    pub(crate) fn read_sdl(&mut self, request: ClipboardRead) -> ClipboardDispatch {
        self.submit(ClipboardRequest::Read(request), Host::sdl)
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn write(&mut self, request: ClipboardWrite) -> ClipboardDispatch {
        self.submit(ClipboardRequest::Write(request), Host::winit)
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn read(&mut self, request: ClipboardRead) -> ClipboardDispatch {
        self.submit(ClipboardRequest::Read(request), Host::winit)
    }

    fn submit(
        &mut self,
        request: ClipboardRequest,
        create: fn() -> Result<Host, ClipboardError>,
    ) -> ClipboardDispatch {
        if self.pending.is_some() || self.completed.is_some() {
            return ClipboardDispatch::Complete(failed(request, ClipboardError::Unavailable));
        }
        if self.host.is_none() {
            match create() {
                Ok(host) => self.host = Some(host),
                Err(error) => return ClipboardDispatch::Complete(failed(request, error)),
            }
        }
        let host = self.host.as_ref().expect("initialized Android clipboard");
        match host.submit(&request) {
            Ok(()) => {
                self.pending = Some(request);
                ClipboardDispatch::Pending
            }
            Err(error) => ClipboardDispatch::Complete(failed(request, error)),
        }
    }

    pub(crate) fn poll(&mut self) -> Option<ClipboardResponse> {
        if let Some(response) = self.completed.take() {
            return Some(response);
        }
        let request = self.pending.as_ref()?;
        let host = self.host.as_ref()?;
        let result = match host.done() {
            Ok(false) => return None,
            Ok(true) => host.take(matches!(request, ClipboardRequest::Read(_))),
            Err(error) => Err(error),
        };
        let request = self
            .pending
            .take()
            .expect("pending Android clipboard request");
        Some(match (request, result) {
            (ClipboardRequest::Read(request), Ok(Some(data))) => request.complete(Ok(data)),
            (ClipboardRequest::Write(request), Ok(None)) => request.complete(Ok(())),
            (request, Err(error)) => {
                self.host = None;
                failed(request, error)
            }
            (request, _) => failed(request, ClipboardError::InvalidData),
        })
    }

    pub(crate) fn detach(&mut self) {
        self.host = None;
        if let Some(request) = self.pending.take() {
            self.completed = Some(failed(request, ClipboardError::Unavailable));
        }
    }
}

fn failed(request: ClipboardRequest, error: ClipboardError) -> ClipboardResponse {
    match request {
        ClipboardRequest::Write(request) => request.complete(Err(error)),
        ClipboardRequest::Read(request) => request.complete(Err(error)),
        #[cfg(test)]
        ClipboardRequest::DeferredRead(request, _) => request.complete(Err(error)),
    }
}
