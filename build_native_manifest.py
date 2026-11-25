"""Pin private native entry points and hardcoded RVAs to the exact, locally verified WPS binaries.

WARNING: Adapting to a newer WPS build MUST re-verify every hardcoded RVA and vtable slot
via disassembly and live readback testing. Never blindly refresh hashes alone.
"""
import hashlib
import json
import re
from pathlib import Path
import pefile

PROJECT = Path(__file__).resolve().parent
ROOT = Path(r'C:\Program Files (x86)\Kingsoft\WPS Office\12.8.2.19823\office6')
OUTPUT = PROJECT / 'native' / 'v3'

# Core shared DLLs loaded by Writer (wps.exe), Presentation (wpp.exe), and Spreadsheets (et.exe)
COMMON_DLLS = (
    'kshell.dll',
    'ksolite.dll',
    'kprometheus.dll',
    'kso.dll',
    'Qt5CoreKso.dll',
    'Qt5GuiKso.dll',
    'Qt5WidgetsKso.dll',
)
WRITER_ONLY_DLLS = ('wpsmain.dll',)
HOST_EXES = ('wps.exe', 'wpp.exe', 'et.exe')
FILES = HOST_EXES + WRITER_ONLY_DLLS + COMMON_DLLS

# Exact byte signatures guarding hardcoded RVAs in display_options.h for WPS 12.8.2.19823 x86:
#   wpsmain.dll + 0x890a0  -> display options context setup prologue
#   wpsmain.dll + 0x8a1adc -> IKApplication options interface IID
WPSMAIN_RVA_SIGNATURES = {
    0x890A0: '558bec6aff68b1c7771064a100000000',
    0x8A1ADC: '59341d7b7c64314eb968965975674158',
}


def digest(path: Path) -> str:
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def verify_rva_signatures() -> None:
    pe = pefile.PE(str(ROOT / 'wpsmain.dll'), fast_load=True)
    try:
        for rva, expected_hex in WPSMAIN_RVA_SIGNATURES.items():
            actual_hex = pe.get_data(rva, len(expected_hex) // 2).hex()
            if actual_hex != expected_hex:
                raise RuntimeError(
                    f'wpsmain.dll + {hex(rva)} byte signature mismatch: '
                    f'expected {expected_hex}, got {actual_hex}. '
                    'Do NOT refresh hashes without re-verifying RVAs via disassembly!'
                )
    finally:
        pe.close()


if __name__ == '__main__':
    verify_rva_signatures()
    exports = set()
    for name in FILES:
        pe = pefile.PE(str(ROOT / name), fast_load=True, max_symbol_exports=100000)
        pe.parse_data_directories(directories=[pefile.DIRECTORY_ENTRY['IMAGE_DIRECTORY_ENTRY_EXPORT']])
        exports.update(
            s.name.decode()
            for s in getattr(pe, 'DIRECTORY_ENTRY_EXPORT', type('Empty', (), {'symbols': []})()).symbols
            if s.name
        )
        pe.close()
    required = set()
    for path in (PROJECT / 'native/v3-src').glob('*'):
        if path.suffix in ('.h', '.cpp'):
            required.update(re.findall(r'"(\?[^"\r\n]+)"', path.read_text('utf-8')))
    missing = required - exports
    if missing:
        raise RuntimeError('WPS native exports missing: ' + ', '.join(sorted(missing)))
    data = {
        'protocol': 4,
        'wps_version': '12.8.2.19823',
        'machine': 'x86',
        'agent_sha256': digest(OUTPUT / 'WpsNativeEyeV3.dll'),
        'common_dlls': list(COMMON_DLLS),
        'writer_dlls': list(WRITER_ONLY_DLLS),
        'host_exes': list(HOST_EXES),
        'wps_files': {name: digest(ROOT / name) for name in FILES},
    }
    (OUTPUT / 'manifest.json').write_text(json.dumps(data, indent=2), encoding='utf-8')
    print('Native v3 (protocol 4, Writer+PPT+ET skin + atomic page options) built and pinned to WPS 12.8.2.19823 x86.')
