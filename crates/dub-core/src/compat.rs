//! The release checks of built and downloaded binaries, one implementation for every program
//! (`engine-build-compat-checklist.md`):
//! - AVX-512 only where the code picks it after a CPUID check: in functions of the libraries that dispatch at run time,
//!   or in a ggml CPU variant built for such a processor and loaded only on one;
//! - Linux libraries need at most GLIBC 2.35 and GLIBCXX 3.4.30 (Ubuntu 22.04), name no absolute RUNPATH and no NCCL;
//! - a program a component unpacks is executable;
//! - on Windows every library a binary imports, delay-loaded ones too, lies beside it (or in a folder the program puts on
//!   its PATH) or is part of Windows or of the display driver.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use iced_x86::{Decoder, DecoderOptions, Instruction};
use object::read::elf::ElfFile64;
use object::read::pe::{PeFile32, PeFile64};
use object::{LittleEndian, Object, ObjectSection, ObjectSymbol, SectionKind, SymbolKind};

/// The newest glibc symbol version a Linux library may need: Ubuntu 22.04's.
pub const GLIBC_MAX: (u32, u32) = (2, 35);
/// The newest libstdc++ symbol version a Linux library may need: GCC 12's, Ubuntu 22.04's.
pub const GLIBCXX_MAX: (u32, u32, u32) = (3, 4, 30);

/// Crates and libraries that choose AVX-512 at run time after a CPUID check, matched as whole crate names in mangled
/// Rust names (`9crc32fast11specialized…`).
const DISPATCHING_CRATES: &[&str] = &["crc32fast", "zlib_rs", "aws_lc_rs", "aws_lc_sys", "ort_sys", "ort", "blake3", "aes"];
/// The same libraries' C and assembly functions, by prefix.
const DISPATCHING_PREFIXES: &[&str] = &["aws_lc_", "_aws_lc_", "blake3_", "_blake3_"];
/// Libraries of Ubuntu 22.04's packages that an AppImage carries in usr/lib and that have AVX-512 code: the distribution
/// runs them on every x86-64 processor, so each picks that code after its own CPUID check. Matched by file name prefix.
const DISTRO_DISPATCHED: &[&str] = &[
    "libcrypto.so", "libdav1d.so", "libdeflate.so", "libgfortran.so", "libjavascriptcoregtk-", "libsodium.so", "libvpx.so",
    "libwebkit2gtk-", "libx264.so", "libx265.so", "libzimg.so",
];
/// ggml's CPU variants for AVX-512 processors: ggml loads one only on a processor that has what it was built for.
const AVX512_GGML_VARIANTS: &[&str] = &["skylakex", "icelake", "cannonlake", "cascadelake", "cooperlake", "zen4", "sapphirerapids"];

/// Libraries every Windows machine has, or that come with the display driver (nvcuda, the Vulkan loader). Everything
/// else is shipped or downloaded beside the binary.
const WINDOWS_LIBRARIES: &[&str] = &[
    "advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "cfgmgr32.dll", "combase.dll", "comctl32.dll", "comdlg32.dll",
    "crypt32.dll", "d2d1.dll", "d3d11.dll", "d3d12.dll", "dbghelp.dll", "dwmapi.dll", "dwrite.dll", "dxcore.dll", "dxgi.dll",
    "gdi32.dll", "imm32.dll", "iphlpapi.dll", "kernel32.dll", "kernelbase.dll", "msvcrt.dll", "mswsock.dll", "ncrypt.dll",
    "netapi32.dll", "ntdll.dll", "ole32.dll", "oleaut32.dll", "pdh.dll", "powrprof.dll", "propsys.dll", "psapi.dll",
    "rpcrt4.dll", "secur32.dll", "setupapi.dll", "shell32.dll", "shlwapi.dll", "uiautomationcore.dll", "user32.dll",
    "userenv.dll", "uxtheme.dll", "version.dll", "windowscodecs.dll", "winhttp.dll", "wininet.dll", "winmm.dll",
    "ws2_32.dll", "opengl32.dll", "rstrtmgr.dll", "mmdevapi.dll", "avrt.dll", "ksuser.dll", "nvcuda.dll", "vulkan-1.dll",
];

/// What a check found wrong in one binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// AVX-512 instructions (count) in a function (or at an address) no CPUID check guards.
    /// `first`: the first such instruction and its address (`vpmullq at 0x180234c1c`).
    Avx512 { place: String, count: usize, first: String },
    /// A symbol version newer than Ubuntu 22.04 has, e.g. `GLIBC_2.38`.
    SymbolVersion(String),
    /// A RUNPATH or RPATH entry that names an absolute folder (the build machine's).
    AbsoluteRunpath(String),
    /// A library of NCCL among what the library needs.
    Nccl(String),
    /// A program without its execute bit.
    NotExecutable,
    /// A library the binary imports that is neither beside it nor part of Windows.
    Unresolved(String),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::Avx512 { place, count, first } => write!(f, "AVX-512 x{count} in {place} without a CPUID dispatcher (first: {first})"),
            Problem::SymbolVersion(v) => write!(f, "needs {v}, newer than Ubuntu 22.04 (GLIBC 2.35, GLIBCXX 3.4.30)"),
            Problem::AbsoluteRunpath(p) => write!(f, "RUNPATH names the absolute folder {p}"),
            Problem::Nccl(n) => write!(f, "needs {n} (NCCL)"),
            Problem::NotExecutable => write!(f, "is a program without its execute bit"),
            Problem::Unresolved(n) => write!(f, "imports {n}, which is neither beside it nor part of Windows"),
        }
    }
}

/// A function's start, size (0 when unknown) and name.
#[derive(Clone, Debug)]
struct Function {
    address: u64,
    size: u64,
    name: String,
}

/// The functions that name the addresses of a binary: its own symbols, or a link map of the build.
#[derive(Clone, Debug, Default)]
pub struct Functions(Vec<Function>);

impl Functions {
    fn new(mut all: Vec<Function>) -> Self {
        all.sort_by_key(|f| f.address);
        Functions(all)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The function an address lies in: a sized one that covers it, else the nearest start at or below it of size
    /// unknown (a label), unless a sized function ends between that start and the address.
    fn at(&self, address: u64) -> Option<&str> {
        let end = self.0.partition_point(|f| f.address <= address);
        let mut label = None;
        for f in self.0[..end].iter().rev() {
            if f.size == 0 {
                label.get_or_insert(f.name.as_str());
            } else if address < f.address + f.size {
                return Some(f.name.as_str());
            } else {
                break;
            }
        }
        label
    }

    /// A link map of the build: MSVC `/MAP`, lld `-Map` (rust-lld) or GNU ld `-Map`.
    pub fn from_link_map(text: &str) -> Self {
        let hex = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok();
        let mut all = Vec::new();
        let mut gnu_pending: Option<String> = None;
        for line in text.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if let Some(name) = gnu_pending.take() {
                if let Some(address) = tokens.first().filter(|t| t.starts_with("0x")).and_then(|t| hex(t)) {
                    let size = tokens.get(1).and_then(|t| hex(t)).unwrap_or(0);
                    all.push(Function { address, size, name });
                    continue;
                }
            }
            // MSVC: " 0001:00012345       name       0000000140013345 f   lib:obj"
            if tokens.len() >= 4 && tokens[3] == "f" && tokens[0].len() == 13 && tokens[0].as_bytes()[4] == b':' && tokens[2].len() == 16 {
                if let Some(address) = hex(tokens[2]) {
                    all.push(Function { address, size: 0, name: tokens[1].to_string() });
                }
                continue;
            }
            // lld: "   17803   17803   15a   1   /x/y.rcgu.o:(.text._RNv...)"
            if tokens.len() >= 5 && tokens[4].contains(":(.text.") {
                if let (Some(address), Some(size)) = (hex(tokens[0]), hex(tokens[2])) {
                    let rest = tokens[4..].join(" ");
                    if let Some(start) = rest.find(":(.text.") {
                        let name = rest[start + 8..].trim_end_matches(')').to_string();
                        all.push(Function { address, size, name });
                    }
                }
                continue;
            }
            // GNU ld: " .text.name 0x12345 0x1a0 obj", or the name alone with the address on the next line
            if line.starts_with(" .text.") {
                let name = tokens[0].trim_start_matches(".text.").to_string();
                match tokens.get(1).filter(|t| t.starts_with("0x")).and_then(|t| hex(t)) {
                    Some(address) => all.push(Function { address, size: tokens.get(2).and_then(|t| hex(t)).unwrap_or(0), name }),
                    None => gnu_pending = Some(name),
                }
            }
        }
        Functions::new(all)
    }
}

/// Whether a function picks AVX-512 after a CPUID check: a function of a dispatching crate or library.
fn dispatches(name: &str) -> bool {
    if DISPATCHING_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return true;
    }
    DISPATCHING_CRATES.iter().any(|krate| {
        name.match_indices(krate).any(|(i, _)| {
            let before = name[..i].chars().next_back();
            let after = name[i + krate.len()..].chars().next();
            before.is_some_and(|c| c.is_ascii_digit()) && after.is_some_and(|c| c.is_ascii_digit())
        })
    })
}

/// A ggml CPU variant for AVX-512 processors (`ggml-cpu-skylakex.dll`, `libggml-cpu-zen4.so`).
fn is_avx512_ggml_variant(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    let stem = name.split('.').next().unwrap_or_default();
    stem.strip_prefix("lib").unwrap_or(stem).strip_prefix("ggml-cpu-").is_some_and(|v| AVX512_GGML_VARIANTS.contains(&v))
}

/// Whether an instruction needs an AVX-512 (or AVX10) processor: any EVEX instruction, on zmm and on xmm and ymm alike.
fn needs_avx512(instruction: &Instruction) -> bool {
    instruction.cpuid_features().iter().any(|f| {
        let name = format!("{f:?}");
        name.starts_with("AVX512") || name.starts_with("AVX10")
    })
}

/// MSVC's level of `__isa_available` for AVX-512: a loop the compiler vectorized for AVX-512 runs behind
/// `cmp dword ptr [__isa_available], 6` and a jump past it to the plain loop when the processor has less.
const MSVC_ISA_AVX512: u64 = 6;

/// A guard of MSVC's vectorized loops: `cmp dword ptr [rip+x], 6` (or more) and, after instructions that leave the flags
/// as they are, `jl`/`jb` forward; the code between the jump and its target runs only on an AVX-512 processor.
/// `cmp`: the compare still deciding the flags at `jump`.
fn isa_guard(cmp: &Instruction, jump: &Instruction) -> Option<(u64, u64)> {
    use iced_x86::{Code, OpKind, Register};
    let compares_memory = matches!(cmp.code(), Code::Cmp_rm32_imm8 | Code::Cmp_rm32_imm32)
        && cmp.op0_kind() == OpKind::Memory
        && cmp.memory_base() == Register::RIP
        && cmp.immediate(1) >= MSVC_ISA_AVX512;
    let below = matches!(jump.code(), Code::Jl_rel8_64 | Code::Jl_rel32_64 | Code::Jb_rel8_64 | Code::Jb_rel32_64);
    (compares_memory && below && jump.near_branch_target() > jump.ip()).then(|| (jump.next_ip(), jump.near_branch_target()))
}

/// The AVX-512 instructions of the executable sections that no MSVC `__isa_available` guard covers: address and
/// mnemonic. The switch tables MSVC keeps in a PE's code (runs of offsets into the code) are skipped, not decoded.
fn avx512_instructions(file: &object::File) -> Vec<(u64, String)> {
    let mut found = Vec::new();
    let mut guarded: Vec<(u64, u64)> = Vec::new();
    let mut instruction = Instruction::default();
    let code: Vec<(u64, u64)> =
        file.sections().filter(|s| s.kind() == SectionKind::Text).map(|s| (s.address(), s.address() + s.size())).collect();
    let pe = matches!(file.format(), object::BinaryFormat::Pe);
    let base = file.relative_address_base();
    for section in file.sections().filter(|s| s.kind() == SectionKind::Text) {
        let Ok(bytes) = section.data() else { continue };
        let mut decoder = Decoder::with_ip(64, bytes, section.address(), DecoderOptions::NONE);
        // the last compare whose flags no instruction since has changed
        let mut flags_from: Option<Instruction> = None;
        while decoder.can_decode() {
            if pe {
                let at = decoder.position();
                let table = switch_table_len(&bytes[at..], section.address() + at as u64, base, &code);
                if table > 0 {
                    decoder.set_position(at + table).expect("inside the section");
                    decoder.set_ip(section.address() + (at + table) as u64);
                    flags_from = None;
                    continue;
                }
            }
            decoder.decode_out(&mut instruction);
            if instruction.is_invalid() {
                flags_from = None;
                continue;
            }
            if let Some(range) = flags_from.as_ref().and_then(|cmp| isa_guard(cmp, &instruction)) {
                guarded.push(range);
            }
            if needs_avx512(&instruction) {
                found.push((instruction.ip(), format!("{:?}", instruction.mnemonic()).to_ascii_lowercase()));
            }
            if instruction.mnemonic() == iced_x86::Mnemonic::Cmp {
                flags_from = Some(instruction);
            } else if instruction.rflags_modified() != 0 || instruction.flow_control() != iced_x86::FlowControl::Next {
                flags_from = None;
            }
        }
    }
    found.retain(|(ip, _)| !guarded.iter().any(|(from, to)| (*from..*to).contains(ip)));
    found
}

/// The fewest offsets in a row taken for a switch table: four dwords that each land in the code are not instructions.
const SWITCH_TABLE_MIN: usize = 4;

/// The bytes of a switch table at `ip` (`bytes` from there): 4-aligned dwords, each an image offset from `base` that
/// lands in one of the code ranges, at least [`SWITCH_TABLE_MIN`] of them; 0 when there is none.
fn switch_table_len(bytes: &[u8], ip: u64, base: u64, code: &[(u64, u64)]) -> usize {
    if ip % 4 != 0 {
        return 0;
    }
    let entries = bytes
        .chunks_exact(4)
        .map(|d| base + u32::from_le_bytes([d[0], d[1], d[2], d[3]]) as u64)
        .take_while(|target| code.iter().any(|(from, to)| (*from..*to).contains(target)))
        .count();
    if entries >= SWITCH_TABLE_MIN {
        entries * 4
    } else {
        0
    }
}

/// The binary's own function symbols (an ELF symbol table; a PE image keeps them in its PDB, so its link map names them),
/// with the untyped labels of code sections that hand-written assembly gives its functions (`blake3_hash_many_avx512`).
fn own_functions(file: &object::File) -> Functions {
    let in_code = |s: &object::Symbol| {
        s.section_index().and_then(|i| file.section_by_index(i).ok()).is_some_and(|section| section.kind() == SectionKind::Text)
    };
    let all = file
        .symbols()
        .chain(file.dynamic_symbols())
        .filter(|s| s.is_definition() && (s.kind() == SymbolKind::Text || (s.kind() == SymbolKind::Unknown && in_code(s))))
        .filter_map(|s| Some(Function { address: s.address(), size: s.size(), name: s.name().ok()?.to_string() }))
        .collect();
    Functions::new(all)
}

/// AVX-512 a binary uses outside the CPUID dispatchers. `map` names the functions of a binary without symbols (a
/// stripped release); with neither, only a ggml AVX-512 variant passes, by its file name.
pub fn avx512(path: &Path, data: &[u8], map: Option<&Functions>) -> Result<Vec<Problem>, String> {
    let file = object::File::parse(data).map_err(|e| format!("{}: {e}", path.display()))?;
    if !matches!(file.architecture(), object::Architecture::X86_64) {
        return Ok(Vec::new());
    }
    if path.file_name().and_then(|n| n.to_str()).is_some_and(is_avx512_ggml_variant) {
        return Ok(Vec::new());
    }
    let instructions = avx512_instructions(&file);
    if instructions.is_empty() {
        return Ok(Vec::new());
    }
    let own = own_functions(&file);
    let names = map.filter(|m| !m.is_empty()).unwrap_or(&own);
    let mut by_place: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (address, mnemonic) in instructions {
        let place = match names.at(address) {
            Some(name) if dispatches(name) => continue,
            Some(name) => name.to_string(),
            None => "code no symbol names".to_string(),
        };
        by_place.entry(place).or_insert_with(|| (0, format!("{mnemonic} at 0x{address:x}"))).0 += 1;
    }
    Ok(by_place.into_iter().map(|(place, (count, first))| Problem::Avx512 { place, count, first }).collect())
}

/// `GLIBC_2.38` → too new; `GLIBCXX_3.4.31` → too new; other versions pass.
fn too_new(version: &str) -> bool {
    let numbers = |rest: &str| rest.split('.').map(|p| p.parse::<u32>().ok()).collect::<Option<Vec<u32>>>();
    if let Some(rest) = version.strip_prefix("GLIBCXX_") {
        return numbers(rest).is_some_and(|n| {
            let (a, b, c) = (n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0), n.get(2).copied().unwrap_or(0));
            (a, b, c) > GLIBCXX_MAX
        });
    }
    if let Some(rest) = version.strip_prefix("GLIBC_") {
        return numbers(rest).is_some_and(|n| (n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0)) > GLIBC_MAX);
    }
    false
}

/// What a 64-bit Linux library needs that Ubuntu 22.04 has not got, an absolute RUNPATH and NCCL.
/// The symbol versions a 64-bit Linux library needs of the libraries it links (`GLIBC_2.34`, `GLIBCXX_3.4.30`), once each.
pub fn needed_versions(path: &Path, data: &[u8]) -> Result<Vec<String>, String> {
    let fail = |e: object::Error| format!("{}: {e}", path.display());
    let file = ElfFile64::<LittleEndian>::parse(data).map_err(fail)?;
    let (endian, sections) = (file.endian(), file.elf_section_table());
    let mut versions = Vec::new();
    if let Some((mut needs, link)) = sections.gnu_verneed(endian, data).map_err(fail)? {
        let strings = sections.strings(endian, data, link).map_err(fail)?;
        while let Some((_, mut aux)) = needs.next().map_err(fail)? {
            while let Some(version) = aux.next().map_err(fail)? {
                let name = String::from_utf8_lossy(version.name(endian, strings).map_err(fail)?).into_owned();
                if !versions.contains(&name) {
                    versions.push(name);
                }
            }
        }
    }
    Ok(versions)
}

pub fn elf(path: &Path, data: &[u8]) -> Result<Vec<Problem>, String> {
    let fail = |e: object::Error| format!("{}: {e}", path.display());
    let file = ElfFile64::<LittleEndian>::parse(data).map_err(fail)?;
    let (endian, sections) = (file.endian(), file.elf_section_table());
    let mut problems: Vec<Problem> = needed_versions(path, data)?.into_iter().filter(|v| too_new(v)).map(Problem::SymbolVersion).collect();
    let table = sections.dynamic_table(endian, data).map_err(fail)?;
    for entry in &table {
        let text = || table.string(entry).map(|s| String::from_utf8_lossy(s).into_owned()).map_err(fail);
        match entry.tag {
            object::elf::DT_RUNPATH | object::elf::DT_RPATH => {
                problems.extend(text()?.split(':').filter(|p| p.starts_with('/')).map(|p| Problem::AbsoluteRunpath(p.to_string())));
            }
            object::elf::DT_NEEDED => {
                let needed = text()?;
                if needed.starts_with("libnccl") {
                    problems.push(Problem::Nccl(needed));
                }
            }
            _ => {}
        }
    }
    Ok(problems)
}

/// The libraries a Windows binary imports, delay-loaded ones too.
pub fn pe_imports(path: &Path, data: &[u8]) -> Result<Vec<String>, String> {
    match object::FileKind::parse(data).map_err(|e| format!("{}: {e}", path.display()))? {
        object::FileKind::Pe32 => imports_of(path, &PeFile32::parse(data).map_err(|e| format!("{}: {e}", path.display()))?),
        _ => imports_of(path, &PeFile64::parse(data).map_err(|e| format!("{}: {e}", path.display()))?),
    }
}

fn imports_of<Pe: object::read::pe::ImageNtHeaders>(path: &Path, file: &object::read::pe::PeFile<'_, Pe>) -> Result<Vec<String>, String> {
    let fail = |e: object::Error| format!("{}: {e}", path.display());
    let mut names = Vec::new();
    if let Some(table) = file.import_table().map_err(fail)? {
        let mut descriptors = table.descriptors().map_err(fail)?;
        while let Some(d) = descriptors.next().map_err(fail)? {
            names.push(String::from_utf8_lossy(table.name(d.name.get(LittleEndian)).map_err(fail)?).into_owned());
        }
    }
    if let Some(table) = file.delay_load_import_table().map_err(fail)? {
        let mut descriptors = table.descriptors().map_err(fail)?;
        while let Some(d) = descriptors.next().map_err(fail)? {
            names.push(String::from_utf8_lossy(table.name(d.dll_name_rva.get(LittleEndian)).map_err(fail)?).into_owned());
        }
    }
    Ok(names)
}

/// Part of Windows or of the display driver.
pub fn windows_has(library: &str) -> bool {
    let name = library.to_ascii_lowercase();
    name.starts_with("api-ms-win-") || name.starts_with("ext-ms-win-") || WINDOWS_LIBRARIES.contains(&name.as_str())
}

/// What `entry` needs, through the libraries it loads, that none of `folders` holds, `provided` does not name (what the
/// program supplies otherwise, such as a VC++ runtime it takes from the system folder) and Windows does not supply: the
/// program's own folder first, then the folders it puts on its PATH.
pub fn unresolved_in(folders: &[PathBuf], provided: &[&str], entry: &Path) -> Result<Vec<String>, String> {
    let mut seen: Vec<String> = Vec::new();
    let mut queue = vec![entry.to_path_buf()];
    let mut missing = Vec::new();
    while let Some(path) = queue.pop() {
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for import in pe_imports(&path, &data)? {
            let lowered = import.to_ascii_lowercase();
            if windows_has(&import) || provided.iter().any(|p| p.eq_ignore_ascii_case(&import)) || seen.contains(&lowered) {
                continue;
            }
            seen.push(lowered);
            match folders.iter().map(|folder| folder.join(&import)).find(|candidate| candidate.is_file()) {
                Some(found) => queue.push(found),
                None => missing.push(import),
            }
        }
    }
    Ok(missing)
}

/// Whether a program has its execute bit (always on systems without one).
pub fn executable(path: &Path) -> Result<bool, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(meta.permissions().mode() & 0o111 == 0o111)
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        Ok(true)
    }
}

/// The kind of binary a file is, by its first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pe,
    Elf,
}

pub fn kind_of(data: &[u8]) -> Option<Kind> {
    match data {
        [b'M', b'Z', ..] => Some(Kind::Pe),
        [0x7f, b'E', b'L', b'F', ..] => Some(Kind::Elf),
        _ => None,
    }
}

/// How a program folder is checked.
#[derive(Default)]
pub struct Tree<'a> {
    /// Folders (relative to the root) the program puts on its PATH, searched after a binary's own folder.
    pub path_folders: &'a [&'a str],
    /// Libraries the program supplies otherwise ([`unresolved_in`]).
    pub provided: &'a [&'a str],
    /// The functions of binaries without symbols, by file name.
    pub maps: BTreeMap<String, Functions>,
    /// File name prefixes (lower case) of vendor libraries whose AVX-512 code the vendor picks at run time and that
    /// carry no symbols to show it (NVIDIA's CUDA libraries, the VC++ runtime).
    pub vendor_dispatched: &'a [&'a str],
    /// File names (lower case) of libraries a package brings that the program never loads (ONNX Runtime's TensorRT
    /// provider beside its CUDA one); they are not checked.
    pub unused: &'a [&'a str],
}

/// The problems of every binary under `root` (ELF and PE, by their first bytes): AVX-512 everywhere, the Linux limits on
/// ELF, unresolved imports on PE.
pub fn check_tree(root: &Path, how: &Tree) -> Result<BTreeMap<PathBuf, Vec<Problem>>, String> {
    let mut found = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let kind = entry.file_type().map_err(|e| format!("{}: {e}", entry.path().display()))?;
            let path = entry.path();
            if kind.is_dir() {
                stack.push(path);
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let Some(problems) = check_file(root, &path, how)? else { continue };
            if !problems.is_empty() {
                found.insert(path, problems);
            }
        }
    }
    Ok(found)
}

/// One binary's problems; None for a file that is not a binary.
pub fn check_file(root: &Path, path: &Path, how: &Tree) -> Result<Option<Vec<Problem>>, String> {
    let mut head = [0u8; 4];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if f.read(&mut head).map_err(|e| format!("{}: {e}", path.display()))? < 4 {
            return Ok(None);
        }
    }
    let Some(kind) = kind_of(&head) else { return Ok(None) };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    let lowered = name.to_ascii_lowercase();
    if how.unused.contains(&lowered.as_str()) {
        return Ok(None);
    }
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut problems = if how.vendor_dispatched.iter().chain(DISTRO_DISPATCHED).any(|p| lowered.starts_with(p)) {
        Vec::new()
    } else {
        avx512(path, &data, how.maps.get(&name))?
    };
    match kind {
        Kind::Elf => problems.extend(elf(path, &data)?),
        Kind::Pe => {
            let mut folders = vec![path.parent().unwrap_or(root).to_path_buf()];
            folders.extend(how.path_folders.iter().map(|f| root.join(f)));
            problems.extend(unresolved_in(&folders, how.provided, path)?.into_iter().map(Problem::Unresolved));
        }
    }
    Ok(Some(problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crates_that_dispatch_are_told_by_their_whole_name() {
        assert!(dispatches("_ZN9crc32fast11specialized6pclmul7calculate17h0123E"));
        assert!(dispatches("_RNvCs1a2b_6blake34hash"), "v0 mangling");
        assert!(dispatches("_blake3_hash_many_avx512"));
        assert!(dispatches("aws_lc_0_32_0_sha256_block_data_order_avx512"));
        assert!(!dispatches("_ZN11sorting_ort4mainE"), "a crate whose name only holds ort");
        assert!(!dispatches("_ZN10dub_server6render4mainE"));
    }

    #[test]
    fn only_the_avx512_variants_of_ggml_pass_by_name() {
        assert!(is_avx512_ggml_variant("ggml-cpu-skylakex.dll"));
        assert!(is_avx512_ggml_variant("libggml-cpu-zen4.so"));
        assert!(!is_avx512_ggml_variant("ggml-cpu-haswell.dll"));
        assert!(!is_avx512_ggml_variant("ggml-cpu.dll"));
    }

    #[test]
    fn versions_newer_than_ubuntu_22_04_are_too_new() {
        assert!(too_new("GLIBC_2.38"));
        assert!(!too_new("GLIBC_2.35"));
        assert!(!too_new("GLIBC_2.2.5"));
        assert!(too_new("GLIBCXX_3.4.31"));
        assert!(!too_new("GLIBCXX_3.4.30"));
        assert!(!too_new("CXXABI_1.3.15"));
    }

    #[test]
    fn link_maps_of_three_linkers_name_their_functions() {
        let msvc = Functions::from_link_map(" 0001:00012345       _ZN9crc32fast4calc17h1E       0000000140013345 f   crc:x.obj\n");
        assert_eq!(msvc.at(0x140013345), Some("_ZN9crc32fast4calc17h1E"));
        let lld = Functions::from_link_map("   17803   17803   15a   1   /t/x.rcgu.o:(.text._ZN4main3run17hE)\n");
        assert_eq!(lld.at(0x17803 + 0x10), Some("_ZN4main3run17hE"));
        assert_eq!(lld.at(0x17803 + 0x15a), None, "past its size");
        let gnu = Functions::from_link_map(" .text._ZN6blake34hash17hE\n                0x0000000000012345      0x1a0 obj\n");
        assert_eq!(gnu.at(0x12350), Some("_ZN6blake34hash17hE"));
    }

    /// A tiny x86-64 ELF executable section, as an object file's would be: `vpxord zmm0, zmm0, zmm0; ret` and `ret`.
    fn elf_with_code(code: &[u8], symbol: &str) -> Vec<u8> {
        use object::write::{Object as Write, StandardSection, Symbol, SymbolSection};
        let mut obj = Write::new(object::BinaryFormat::Elf, object::Architecture::X86_64, object::Endianness::Little);
        let text = obj.section_id(StandardSection::Text);
        let offset = obj.append_section_data(text, code, 16);
        obj.add_symbol(Symbol {
            name: symbol.as_bytes().to_vec(),
            value: offset,
            size: code.len() as u64,
            kind: SymbolKind::Text,
            scope: object::SymbolScope::Linkage,
            weak: false,
            section: SymbolSection::Section(text),
            flags: object::SymbolFlags::None,
        });
        obj.write().unwrap()
    }

    const VPXORD_ZMM0_RET: &[u8] = &[0x62, 0xf1, 0x7d, 0x48, 0xef, 0xc0, 0xc3];
    const VPXORD_XMM0_EVEX_RET: &[u8] = &[0x62, 0xf1, 0x7d, 0x08, 0xef, 0xc0, 0xc3];
    const VPXOR_YMM0_VEX_RET: &[u8] = &[0xc5, 0xfd, 0xef, 0xc0, 0xc3];

    #[test]
    fn avx512_outside_a_dispatcher_is_named_and_inside_one_passes() {
        let p = Path::new("x.o");
        let plain = elf_with_code(VPXORD_ZMM0_RET, "_ZN10dub_server4mixE");
        assert_eq!(avx512(p, &plain, None).unwrap(), vec![Problem::Avx512 { place: "_ZN10dub_server4mixE".into(), count: 1, first: "vpxord at 0x0".into() }]);
        let evex_xmm = elf_with_code(VPXORD_XMM0_EVEX_RET, "_ZN10dub_server4mixE");
        assert_eq!(avx512(p, &evex_xmm, None).unwrap().len(), 1, "EVEX on xmm needs AVX-512 too");
        assert!(avx512(p, &elf_with_code(VPXOR_YMM0_VEX_RET, "_ZN10dub_server4mixE"), None).unwrap().is_empty(), "AVX2 passes");
        assert!(avx512(p, &elf_with_code(VPXORD_ZMM0_RET, "_ZN9crc32fast4calc17h1E"), None).unwrap().is_empty());
        assert!(avx512(Path::new("ggml-cpu-zen4.so"), &plain, None).unwrap().is_empty());
        let map = Functions::from_link_map(" .text._ZN6blake34hash17hE 0x0 0x40 obj\n");
        assert!(avx512(p, &plain, Some(&map)).unwrap().is_empty(), "the link map names it");
        // cmp dword ptr [rip+0x100], 6; jl past the loop; the AVX-512 loop; ret
        let guarded: Vec<u8> = [&[0x83, 0x3d, 0x00, 0x01, 0x00, 0x00, 0x06, 0x7c, 0x06][..], &VPXORD_ZMM0_RET[..6], &[0xc3]].concat();
        assert!(avx512(p, &elf_with_code(&guarded, "_ZN10dub_server4mixE"), None).unwrap().is_empty(), "MSVC's __isa_available guard");
        // the same with `mov [rbp+10h], rbx` between the compare and the jump, which leaves the flags as they are
        let moved: Vec<u8> = [&[0x83, 0x3d, 0x00, 0x01, 0x00, 0x00, 0x06, 0x48, 0x89, 0x5d, 0x10, 0x7c, 0x06][..], &VPXORD_ZMM0_RET[..6], &[0xc3]].concat();
        assert!(avx512(p, &elf_with_code(&moved, "_ZN10dub_server4mixE"), None).unwrap().is_empty(), "a move between keeps the guard");
        // `add eax, 1` between sets the flags anew: the jump no longer follows the compare
        let added: Vec<u8> = [&[0x83, 0x3d, 0x00, 0x01, 0x00, 0x00, 0x06, 0x83, 0xc0, 0x01, 0x7c, 0x06][..], &VPXORD_ZMM0_RET[..6], &[0xc3]].concat();
        assert_eq!(avx512(p, &elf_with_code(&added, "_ZN10dub_server4mixE"), None).unwrap().len(), 1, "an add between ends the guard");
        let unguarded: Vec<u8> = [&[0x83, 0x3d, 0x00, 0x01, 0x00, 0x00, 0x05, 0x7c, 0x06][..], &VPXORD_ZMM0_RET[..6], &[0xc3]].concat();
        assert_eq!(avx512(p, &elf_with_code(&unguarded, "_ZN10dub_server4mixE"), None).unwrap().len(), 1, "a guard for AVX2 is not one for AVX-512");
    }

    /// A linked, unstripped Linux program (gcc -O2, PIE; `testdata/avx512-pie.c`): its symbol table names the global
    /// and the local function the AVX-512 lies in.
    #[test]
    fn an_unstripped_linux_program_names_its_avx512_functions() {
        let data = include_bytes!("../testdata/avx512-pie.elf");
        let found = avx512(Path::new("avx512-pie"), data, None).unwrap();
        let places: Vec<&str> = found.iter().map(|p| match p {
            Problem::Avx512 { place, .. } => place.as_str(),
            other => panic!("{other}"),
        }).collect();
        assert_eq!(places, vec!["add16", "local_add"], "{found:?}");
    }

    /// MSVC's switch tables in a PE's code: four or more aligned offsets into the code are skipped; fewer, an offset
    /// outside the code or an unaligned start are decoded as instructions.
    #[test]
    fn switch_tables_in_pe_code_are_not_instructions() {
        let (base, code) = (0x1_4000_0000u64, [(0x1_4000_1000u64, 0x1_4000_2000u64)]);
        let table = |rvas: &[u32]| rvas.iter().flat_map(|r| r.to_le_bytes()).collect::<Vec<u8>>();
        assert_eq!(switch_table_len(&table(&[0x1010, 0x1020, 0x1030, 0x1040, 0x9999_0000]), 0x1_4000_1800, base, &code), 16);
        assert_eq!(switch_table_len(&table(&[0x1010, 0x1020, 0x1030]), 0x1_4000_1800, base, &code), 0, "three are not a table");
        assert_eq!(switch_table_len(&table(&[0x1010, 0x3000, 0x1030, 0x1040]), 0x1_4000_1800, base, &code), 0, "an offset out of the code");
        assert_eq!(switch_table_len(&table(&[0x1010, 0x1020, 0x1030, 0x1040]), 0x1_4000_1802, base, &code), 0, "unaligned");
    }

    /// DUB_COMPAT_AVX512: a binary; prints the places of its AVX-512 outside the dispatchers.
    #[test]
    #[ignore]
    fn a_binary_names_its_avx512_places() {
        let Ok(path) = std::env::var("DUB_COMPAT_AVX512") else { return };
        let path = PathBuf::from(path);
        let data = std::fs::read(&path).unwrap();
        for problem in avx512(&path, &data, None).unwrap() {
            println!("[..] {problem}");
        }
    }

    /// DUB_COMPAT_TREE: a program folder (an extracted AppDir); prints every binary's problems under the default checks.
    #[test]
    #[ignore]
    fn a_folder_names_its_problems() {
        let Ok(root) = std::env::var("DUB_COMPAT_TREE") else { return };
        let root = PathBuf::from(root);
        for (path, problems) in check_tree(&root, &Tree::default()).unwrap() {
            for problem in problems {
                println!("[..] {}: {problem}", path.strip_prefix(&root).unwrap_or(&path).display());
            }
        }
    }

    /// DUB_COMPAT_ELF: a Linux library; prints the symbol versions it needs and its problems.
    #[test]
    #[ignore]
    fn a_linux_library_names_the_versions_it_needs() {
        let Ok(path) = std::env::var("DUB_COMPAT_ELF") else { return };
        let path = PathBuf::from(path);
        let data = std::fs::read(&path).unwrap();
        let versions = needed_versions(&path, &data).unwrap();
        println!("[..] {}: {}", path.display(), versions.join(" "));
        assert!(versions.iter().any(|v| v.starts_with("GLIBC_")), "a library linked against glibc names its versions");
        for problem in elf(&path, &data).unwrap() {
            println!("[ERROR] {problem}");
        }
    }

    #[test]
    fn a_folder_check_skips_what_is_never_loaded_and_trusts_vendor_dispatch_by_name() {
        let dir = std::env::temp_dir().join(format!("dub-compat-t-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let code = elf_with_code(VPXORD_ZMM0_RET, "f");
        for name in ["libmine.so", "libcublas.so.13", "libunused.so", "libx265.so.199"] {
            std::fs::write(dir.join(name), &code).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"text").unwrap();
        let how = Tree { vendor_dispatched: &["libcublas"], unused: &["libunused.so"], ..Tree::default() };
        let found = check_tree(&dir, &how).unwrap();
        assert_eq!(found.keys().map(|p| p.file_name().unwrap().to_str().unwrap()).collect::<Vec<_>>(), vec!["libmine.so"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_binary_without_its_libraries_is_told_what_is_missing() {
        let dir = std::env::temp_dir().join(format!("dub-compat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = std::env::current_exe().unwrap();
        if kind_of(&std::fs::read(&exe).unwrap()) != Some(Kind::Pe) {
            std::fs::remove_dir_all(&dir).ok();
            return;
        }
        let imports = pe_imports(&exe, &std::fs::read(&exe).unwrap()).unwrap();
        assert!(imports.iter().any(|i| i.eq_ignore_ascii_case("kernel32.dll")), "{imports:?}");
        let foreign: Vec<String> = imports.iter().filter(|i| !windows_has(i)).cloned().collect();
        assert_eq!(unresolved_in(&[dir.clone()], &[], &exe).unwrap(), foreign, "the test binary's VC++ runtime is not beside it here");
        let named: Vec<&str> = foreign.iter().map(String::as_str).collect();
        assert!(unresolved_in(&[dir.clone()], &named, &exe).unwrap().is_empty(), "what the program supplies otherwise");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_program_without_its_execute_bit_is_found() {
        let dir = std::env::temp_dir().join(format!("dub-compat-x-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("audiocpp_server");
        std::fs::write(&program, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(!executable(&program).unwrap());
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(executable(&program).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }
}
