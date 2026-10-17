use alloc::vec::Vec;
use core::ptr;
use windows::{
    Win32::{
        Foundation::{
            E_INVALIDARG, E_OUTOFMEMORY, RPC_E_CHANGED_MODE, WINCODEC_ERR_UNSUPPORTEDOPERATION,
            WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT,
        },
        Graphics::Imaging::*,
        System::Com::*,
        UI::Shell::SHCreateMemStream,
    },
    core::{Error, GUID, Interface, Result},
};

use super::bitmap::V5_HEADER;
use crate::ui::clipboard::ClipboardError;

struct Apartment(bool);

impl Apartment {
    fn enter() -> Result<Self> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self(false));
        }
        result.ok()?;
        Ok(Self(true))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

fn factory() -> Result<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
}

fn stream(bytes: &[u8]) -> Result<IStream> {
    u32::try_from(bytes.len()).map_err(|_| Error::from(E_OUTOFMEMORY))?;
    unsafe { SHCreateMemStream(Some(bytes)) }.ok_or_else(|| E_OUTOFMEMORY.into())
}

fn decode(
    factory: &IWICImagingFactory,
    bytes: &[u8],
    kind: &GUID,
) -> Result<IWICBitmapFrameDecode> {
    let input = stream(bytes)?;
    unsafe {
        let decoder = factory.CreateDecoder(kind, ptr::null())?;
        decoder.Initialize(&input, WICDecodeMetadataCacheOnLoad)?;
        decoder.GetFrame(0)
    }
}

fn pixels(source: &IWICBitmapSource) -> Result<(u32, u32)> {
    let (mut width, mut height) = (0, 0);
    unsafe {
        source.GetSize(&mut width, &mut height)?;
    }
    if width == 0
        || height == 0
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || width
            .checked_mul(4)
            .and_then(|stride| stride.checked_mul(height))
            .is_none()
    {
        return Err(E_INVALIDARG.into());
    }
    Ok((width, height))
}

fn color_context(
    factory: &IWICImagingFactory,
    frame: &IWICBitmapFrameDecode,
) -> Result<Option<IWICColorContext>> {
    let mut count = 0;
    unsafe {
        // WIC's count-only query requires a null pointer, not an empty Rust slice.
        (frame.vtable().GetColorContexts)(frame.as_raw(), 0, ptr::null_mut(), &mut count).ok()?;
        if count == 0 {
            return Ok(None);
        }
        if count != 1 {
            return Err(WINCODEC_ERR_UNSUPPORTEDOPERATION.into());
        }
        let mut contexts = [Some(factory.CreateColorContext()?)];
        frame.GetColorContexts(&mut contexts, &mut count)?;
        if count != 1 {
            return Err(E_INVALIDARG.into());
        }
        Ok(contexts[0].take())
    }
}

pub(super) fn png_to_dib(bytes: &[u8]) -> core::result::Result<Vec<u8>, ClipboardError> {
    let _apartment = Apartment::enter().map_err(super::native_error)?;
    let factory = factory().map_err(super::native_error)?;
    let frame = decode(&factory, bytes, &GUID_ContainerFormatPng).map_err(super::native_error)?;
    let (width, height) = pixels(&frame).map_err(super::native_error)?;
    let mut dib = super::bitmap::bgra_storage(width, height)?;
    unsafe {
        if let Some(context) = color_context(&factory, &frame).map_err(super::native_error)? {
            let destination = factory.CreateColorContext().map_err(super::native_error)?;
            destination
                .InitializeFromExifColorSpace(1)
                .map_err(super::native_error)?;
            let converted = factory
                .CreateColorTransformer()
                .map_err(super::native_error)?;
            converted
                .Initialize(
                    &frame,
                    &context,
                    &destination,
                    &GUID_WICPixelFormat32bppBGRA,
                )
                .map_err(super::native_error)?;
            converted
                .CopyPixels(ptr::null(), width * 4, &mut dib[V5_HEADER..])
                .map_err(super::native_error)?;
        } else {
            let source = WICConvertBitmapSource(&GUID_WICPixelFormat32bppBGRA, &frame)
                .map_err(super::native_error)?;
            source
                .CopyPixels(ptr::null(), width * 4, &mut dib[V5_HEADER..])
                .map_err(super::native_error)?;
        }
    }
    Ok(dib)
}

pub(super) fn dib_to_png(bytes: &[u8]) -> core::result::Result<Vec<u8>, ClipboardError> {
    let bmp = super::bitmap::to_bmp(bytes)?;
    encode_png(&bmp, bytes).map_err(super::native_error)
}

fn encode_png(bmp: &[u8], dib: &[u8]) -> Result<Vec<u8>> {
    let _apartment = Apartment::enter()?;
    let factory = factory()?;
    let decoded = decode(&factory, bmp, &GUID_ContainerFormatBmp)?;
    let (width, height) = pixels(&decoded)?;
    unsafe {
        if dib.len() >= 56
            && u32::from_le_bytes(dib[..4].try_into().unwrap()) >= 108
            && dib[52..56] != [0; 4]
            && decoded.GetPixelFormat()? != GUID_WICPixelFormat32bppBGRA
        {
            return Err(WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT.into());
        }
        let output = SHCreateMemStream(None).ok_or(E_OUTOFMEMORY)?;
        let encoder = factory.CreateEncoder(&GUID_ContainerFormatPng, ptr::null())?;
        encoder.Initialize(&output, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        let mut options = None;
        encoder.CreateNewFrame(&mut frame, &mut options)?;
        let frame = frame.ok_or(E_OUTOFMEMORY)?;
        frame.Initialize(options.as_ref())?;
        frame.SetSize(width, height)?;
        let header = u32::from_le_bytes(dib[..4].try_into().unwrap());
        if header >= 108
            && !matches!(
                u32::from_le_bytes(dib[56..60].try_into().unwrap()),
                0x7352_4742 | 0x5769_6e20
            )
        {
            let context =
                color_context(&factory, &decoded)?.ok_or(WINCODEC_ERR_UNSUPPORTEDOPERATION)?;
            frame.SetColorContexts(&[Some(context)])?;
        }
        let mut format = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut format)?;
        let source = WICConvertBitmapSource(&format, &decoded)?;
        frame.WriteSource(&source, ptr::null())?;
        frame.Commit()?;
        encoder.Commit()?;
        let mut stat = STATSTG::default();
        output.Stat(&mut stat, STATFLAG_NONAME)?;
        let size = u32::try_from(stat.cbSize).map_err(|_| Error::from(E_OUTOFMEMORY))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size as usize)
            .map_err(|_| Error::from(E_OUTOFMEMORY))?;
        bytes.resize(size as usize, 0);
        output.Seek(0, STREAM_SEEK_SET, None)?;
        let mut read = 0;
        output
            .Read(bytes.as_mut_ptr().cast(), size, Some(&mut read))
            .ok()?;
        if read != size {
            return Err(E_INVALIDARG.into());
        }
        Ok(bytes)
    }
}
