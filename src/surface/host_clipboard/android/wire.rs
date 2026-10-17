use alloc::vec::Vec;
use jni::{
    Env, JValue, jni_sig, jni_str,
    objects::{JByteArray, JClass, JObject, JObjectArray, JString},
    refs::{Global, IntoAuto},
};

use crate::ui::clipboard::{ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem};

type MimeItems<'a> = JObjectArray<'a, JObjectArray<'a, JString<'a>>>;
type ByteItems<'a> = JObjectArray<'a, JObjectArray<'a, JByteArray<'a>>>;

pub(super) enum Failure {
    Jni(jni::errors::Error),
    Data(ClipboardError),
}

impl From<jni::errors::Error> for Failure {
    fn from(error: jni::errors::Error) -> Self {
        Self::Jni(error)
    }
}

impl From<ClipboardError> for Failure {
    fn from(error: ClipboardError) -> Self {
        Self::Data(error)
    }
}

impl Failure {
    pub(super) fn clipboard(self) -> ClipboardError {
        match self {
            Self::Data(error) => error,
            Self::Jni(jni::errors::Error::CaughtJavaException { name, .. }) => {
                match name.as_str() {
                    "java.lang.SecurityException" => ClipboardError::PermissionDenied,
                    "java.lang.OutOfMemoryError" => ClipboardError::Capacity,
                    "java.lang.UnsupportedOperationException"
                    | "java.lang.ClassNotFoundException" => ClipboardError::Unsupported,
                    "java.lang.IllegalArgumentException"
                    | "java.nio.charset.MalformedInputException"
                    | "java.nio.charset.UnmappableCharacterException" => {
                        ClipboardError::InvalidData
                    }
                    "java.io.IOException"
                    | "java.io.FileNotFoundException"
                    | "android.os.OperationCanceledException"
                    | "java.util.concurrent.CancellationException"
                    | "java.lang.IllegalStateException" => ClipboardError::Unavailable,
                    _ => ClipboardError::Platform,
                }
            }
            Self::Jni(_) => ClipboardError::Platform,
        }
    }
}

pub(super) fn encode<'local>(
    env: &mut Env<'local>,
    class: &Global<JClass<'static>>,
    data: &ClipboardData,
) -> Result<JObject<'local>, Failure> {
    data.validate()?;
    let types = MimeItems::new(env, data.items().len(), JObjectArray::<JString>::null())?;
    let values = ByteItems::new(env, data.items().len(), JObjectArray::<JByteArray>::null())?;
    for (index, item) in data.items().iter().enumerate() {
        let names =
            JObjectArray::<JString>::new(env, item.formats().len(), JString::null())?.auto();
        let bytes =
            JObjectArray::<JByteArray>::new(env, item.formats().len(), JByteArray::null())?.auto();
        for (column, format) in item.formats().iter().enumerate() {
            let name = env.new_string(format.mime())?.auto();
            let value = env.byte_array_from_slice(format.bytes())?.auto();
            names.set_element(env, column, &*name)?;
            bytes.set_element(env, column, &*value)?;
        }
        types.set_element(env, index, &*names)?;
        values.set_element(env, index, &*bytes)?;
    }
    Ok(env.new_object(
        class,
        jni_sig!("([[Ljava/lang/String;[[[B)V"),
        &[JValue::Object(&types), JValue::Object(&values)],
    )?)
}

pub(super) fn decode(env: &mut Env<'_>, payload: JObject<'_>) -> Result<ClipboardData, Failure> {
    let types = env
        .get_field(
            &payload,
            jni_str!("types"),
            jni_sig!("[[Ljava/lang/String;"),
        )?
        .l()?;
    let values = env
        .get_field(&payload, jni_str!("values"), jni_sig!("[[[B"))?
        .l()?;
    let types = env.cast_local::<MimeItems>(types)?;
    let values = env.cast_local::<ByteItems>(values)?;
    let count = types.len(env)?;
    if count != values.len(env)? {
        return Err(ClipboardError::InvalidData.into());
    }
    let mut data = ClipboardData::with_capacity(count, usize::MAX)?;
    for index in 0..count {
        let names = types.get_element(env, index)?.auto();
        let bytes = values.get_element(env, index)?.auto();
        let formats = names.len(env)?;
        if formats != bytes.len(env)? {
            return Err(ClipboardError::InvalidData.into());
        }
        let mut item = ClipboardItem::with_capacity(formats)?;
        for column in 0..formats {
            let name = names.get_element(env, column)?.auto();
            let value = bytes.get_element(env, column)?.auto();
            let length = value.len(env)?;
            let mut copied = Vec::new();
            copied
                .try_reserve_exact(length)
                .map_err(|_| ClipboardError::Capacity)?;
            let mut buffer = [0i8; 8192];
            while copied.len() < length {
                let count = buffer.len().min(length - copied.len());
                let offset = i32::try_from(copied.len()).map_err(|_| ClipboardError::Capacity)?;
                value.get_region(env, offset, &mut buffer[..count])?;
                copied.extend(buffer[..count].iter().map(|byte| *byte as u8));
            }
            item.push(ClipboardFormat::new(name.try_to_string(env)?, copied))?;
        }
        data.push(item)?;
    }
    data.validate()?;
    Ok(data)
}
