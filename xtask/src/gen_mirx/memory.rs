use mirx::{
    ByteAlignment,
    image::{CacheSync, DecodeRequest, MemoryPlacement, SurfaceRequirements},
};

use super::Result;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DecodeArgs {
    output_alignment: ByteAlignment,
    plane_alignment: ByteAlignment,
    width_multiple: u32,
    height_multiple: u32,
    stride_multiple: u32,
    workspace_alignment: ByteAlignment,
    input: MemoryPlacement,
    output: MemoryPlacement,
    workspace: MemoryPlacement,
}

impl DecodeArgs {
    pub(super) fn parse(&mut self, option: &str, value: &str) -> Result<bool> {
        match option {
            "--output-align" => {
                self.output_alignment = alignment(value, option)?;
            }
            "--plane-align" => {
                self.plane_alignment = alignment(value, option)?;
            }
            "--width-multiple" => {
                self.width_multiple = multiple(value, option)?;
            }
            "--height-multiple" => {
                self.height_multiple = multiple(value, option)?;
            }
            "--stride-multiple" => {
                self.stride_multiple = multiple(value, option)?;
            }
            "--workspace-align" => {
                self.workspace_alignment = alignment(value, option)?;
            }
            "--input-memory" => self.input = placement(value)?,
            "--output-memory" => self.output = placement(value)?,
            "--workspace-memory" => self.workspace = placement(value)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn request(self) -> Result<DecodeRequest> {
        let request = DecodeRequest::new(
            SurfaceRequirements::new()
                .with_base_alignment(self.output_alignment)
                .with_plane_alignment(self.plane_alignment)
                .with_width_multiple(self.width_multiple)
                .with_height_multiple(self.height_multiple)
                .with_stride_multiple(self.stride_multiple),
        )
        .with_input(self.input)
        .with_output(self.output)
        .with_workspace(self.workspace)
        .with_workspace_alignment(self.workspace_alignment);
        request
            .validate_reconstruction()
            .map_err(|error| format!("unsupported decode memory contract: {error:?}"))?;
        Ok(request)
    }
}

impl Default for DecodeArgs {
    fn default() -> Self {
        Self {
            output_alignment: ByteAlignment::ONE,
            plane_alignment: ByteAlignment::ONE,
            width_multiple: 1,
            height_multiple: 1,
            stride_multiple: 1,
            workspace_alignment: ByteAlignment::ONE,
            input: MemoryPlacement::Cpu,
            output: MemoryPlacement::Cpu,
            workspace: MemoryPlacement::Cpu,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ContractReport {
    request: DecodeRequest,
    input_alignment: ByteAlignment,
    input_addresses_aligned: bool,
}

impl ContractReport {
    pub(super) const fn new(
        request: DecodeRequest,
        input_alignment: ByteAlignment,
        input_addresses_aligned: bool,
    ) -> Self {
        Self {
            request,
            input_alignment,
            input_addresses_aligned,
        }
    }

    pub(super) const fn request(self) -> DecodeRequest {
        self.request
    }

    pub(super) const fn input_alignment(self) -> ByteAlignment {
        self.input_alignment
    }

    pub(super) const fn input_addresses_are_aligned(self) -> bool {
        self.input_addresses_aligned
    }

    pub(super) fn print(self) {
        println!(
            "  input: stored alignment {} B, host slice {}",
            self.input_alignment(),
            if self.input_addresses_are_aligned() {
                "aligned"
            } else {
                "misaligned"
            },
        );
        println!(
            "  memory: input {}, output {}, workspace {}; cache input {}, output {}",
            placement_name(self.request.input()),
            placement_name(self.request.output()),
            placement_name(self.request.workspace()),
            sync_name(self.request.input_sync()),
            sync_name(self.request.output_sync()),
        );
    }
}

fn alignment(value: &str, option: &str) -> Result<ByteAlignment> {
    let value = value
        .parse::<u32>()
        .map_err(|_| format!("{option} must be a positive power of two"))?;
    ByteAlignment::new(value)
        .map_err(|_| format!("{option} must be a positive power of two").into())
}

fn multiple(value: &str, option: &str) -> Result<u32> {
    let value = value
        .parse::<u32>()
        .map_err(|_| format!("{option} must be positive"))?;
    if value == 0 {
        return Err(format!("{option} must be positive").into());
    }
    Ok(value)
}

fn placement(value: &str) -> Result<MemoryPlacement> {
    match value.to_ascii_lowercase().as_str() {
        "cpu" => Ok(MemoryPlacement::Cpu),
        "flash" => Ok(MemoryPlacement::Flash),
        "shared-coherent" => Ok(MemoryPlacement::SharedCoherent),
        "shared-noncoherent" => Ok(MemoryPlacement::SharedNoncoherent),
        "device" => Ok(MemoryPlacement::Device),
        _ => Err(format!(
            "invalid memory placement {value:?}; expected cpu, flash, shared-coherent, shared-noncoherent, or device"
        )
        .into()),
    }
}

fn placement_name(placement: MemoryPlacement) -> &'static str {
    match placement {
        MemoryPlacement::Cpu => "cpu",
        MemoryPlacement::Flash => "flash",
        MemoryPlacement::SharedCoherent => "shared-coherent",
        MemoryPlacement::SharedNoncoherent => "shared-noncoherent",
        MemoryPlacement::Device => "device",
    }
}

fn sync_name(sync: CacheSync) -> &'static str {
    match sync {
        CacheSync::None => "none",
        CacheSync::InvalidateBeforeRead => "invalidate-before-read",
        CacheSync::CleanAfterWrite => "clean-after-write",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_shared_decode_contract() {
        let mut args = DecodeArgs::default();
        for (option, value) in [
            ("--output-align", "64"),
            ("--plane-align", "32"),
            ("--width-multiple", "8"),
            ("--height-multiple", "2"),
            ("--stride-multiple", "64"),
            ("--workspace-align", "128"),
            ("--input-memory", "flash"),
            ("--output-memory", "shared-noncoherent"),
            ("--workspace-memory", "shared-coherent"),
        ] {
            assert!(args.parse(option, value).unwrap());
        }
        assert!(!args.parse("--other", "1").unwrap());
        let request = args.request().unwrap();
        assert_eq!(request.requirements().base_alignment(), 64);
        assert_eq!(request.requirements().plane_alignment(), 32);
        assert_eq!(request.requirements().width_multiple(), 8);
        assert_eq!(request.requirements().height_multiple(), 2);
        assert_eq!(request.requirements().stride_multiple(), 64);
        assert_eq!(request.workspace_alignment(), 128);
        assert_eq!(request.input(), MemoryPlacement::Flash);
        assert_eq!(request.output(), MemoryPlacement::SharedNoncoherent);
        assert_eq!(request.workspace(), MemoryPlacement::SharedCoherent);
    }

    #[test]
    fn rejects_invalid_geometry_and_inaccessible_memory() {
        for (option, value) in [
            ("--output-align", "3"),
            ("--plane-align", "0"),
            ("--width-multiple", "0"),
            ("--height-multiple", "no"),
            ("--stride-multiple", "0"),
            ("--workspace-align", "6"),
            ("--input-memory", "unknown"),
        ] {
            assert!(DecodeArgs::default().parse(option, value).is_err());
        }

        let mut output = DecodeArgs::default();
        output.parse("--output-memory", "device").unwrap();
        assert!(output.request().is_err());

        let mut workspace = DecodeArgs::default();
        workspace.parse("--workspace-memory", "flash").unwrap();
        assert!(workspace.request().is_err());
    }
}
