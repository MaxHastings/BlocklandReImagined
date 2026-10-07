"""Fail when a Windows executable needs a DLL a fresh Windows PC lacks.

The release links the C runtime statically (.cargo/config.toml); this reads
each executable's import table and refuses the Visual C++ runtime DLLs, so a
build that lost that setting cannot ship.

    python tools/check_windows_imports.py bri-client.exe bri-server.exe ...
"""
import struct
import sys
from pathlib import Path

# The Visual C++ Redistributable's DLLs. The universal C runtime
# (api-ms-win-crt-*, ucrtbase.dll) is part of Windows 10 and 11.
REDISTRIBUTABLE_PREFIXES = ('vcruntime', 'msvcp', 'concrt', 'vccorlib')

PE_SIGNATURE = b'PE\0\0'
PE_OFFSET_FIELD = 0x3C
COFF_HEADER_SIZE = 20
OPTIONAL_MAGIC_PE32_PLUS = 0x20B
# Offset of the data directories in a PE32+ optional header, and the size of
# one directory entry (RVA, size) and one import descriptor.
DATA_DIRECTORIES_PE32_PLUS = 112
DIRECTORY_ENTRY_SIZE = 8
IMPORT_DIRECTORY_INDEX = 1
IMPORT_DESCRIPTOR_SIZE = 20
IMPORT_NAME_FIELD = 12
SECTION_HEADER_SIZE = 40


def imports(path: Path) -> list[str]:
    data = path.read_bytes()
    pe = struct.unpack_from('<I', data, PE_OFFSET_FIELD)[0]
    if data[pe:pe + len(PE_SIGNATURE)] != PE_SIGNATURE:
        sys.exit(f'{path}: not a PE executable')
    coff = pe + len(PE_SIGNATURE)
    sections, optional_size = struct.unpack_from('<H12xH', data, coff + 2)
    optional = coff + COFF_HEADER_SIZE
    if struct.unpack_from('<H', data, optional)[0] != OPTIONAL_MAGIC_PE32_PLUS:
        sys.exit(f'{path}: not a 64-bit executable')
    directory = optional + DATA_DIRECTORIES_PE32_PLUS + IMPORT_DIRECTORY_INDEX * DIRECTORY_ENTRY_SIZE
    import_rva = struct.unpack_from('<I', data, directory)[0]
    table = optional + optional_size
    # Each section: its size and address in memory, its size and offset in
    # the file.
    headers = [struct.unpack_from('<8xIIII', data, table + i * SECTION_HEADER_SIZE)
               for i in range(sections)]

    def offset(rva: int) -> int:
        for size, address, _raw_size, raw in headers:
            if address <= rva < address + size:
                return rva - address + raw
        sys.exit(f'{path}: address {rva:#x} is in no section')

    names = []
    descriptor = offset(import_rva)
    while True:
        name_rva = struct.unpack_from('<I', data, descriptor + IMPORT_NAME_FIELD)[0]
        if name_rva == 0:
            return names
        start = offset(name_rva)
        names.append(data[start:data.index(b'\0', start)].decode('ascii'))
        descriptor += IMPORT_DESCRIPTOR_SIZE


def main() -> None:
    failed = False
    for argument in sys.argv[1:]:
        needed = [name for name in imports(Path(argument))
                  if name.lower().startswith(REDISTRIBUTABLE_PREFIXES)]
        if needed:
            failed = True
            print(f'{argument} needs the Visual C++ Redistributable: {", ".join(needed)}')
        else:
            print(f'{argument}: no Visual C++ Redistributable DLLs')
    if failed:
        sys.exit('Link the C runtime statically (.cargo/config.toml, target-feature=+crt-static).')


if __name__ == '__main__':
    main()
