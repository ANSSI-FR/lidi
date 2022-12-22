#!/usr/bin/env python3
import re
import sys
import pathlib

# Base path for Linux Kernel sources
BASE = (sys.argv[1] if len(sys.argv) > 1 else '.')

SOURCES = [
    ('b32/arm',         {'common'},                 'arch/arm/tools/syscall.tbl'),
    ('b32/sparc',       {'common', '32'},           'arch/sparc/kernel/syscalls/syscall.tbl'),
    ('b32/x86',         {'i386'},                   'arch/x86/entry/syscalls/syscall_32.tbl'),
    ('b32/powerpc',     {'common', 'nospu', '32'},  'arch/powerpc/kernel/syscalls/syscall.tbl'),
    ('b32/mips',        {'o32'},                    'arch/mips/kernel/syscalls/syscall_o32.tbl'),
    ('b64/x86_64',      {'common', '64'},           'arch/x86/entry/syscalls/syscall_64.tbl'),
    ('b64/powerpc64',   {'common', 'nospu', '64'},  'arch/powerpc/kernel/syscalls/syscall.tbl'),
    ('b64/s390x',       {'common', '64'},           'arch/s390/kernel/syscalls/syscall.tbl'),
    ('b64/sparc64',     {'common', '64'},           'arch/sparc/kernel/syscalls/syscall.tbl'),
    ('b64/mips64',      {'n64'},                    'arch/mips/kernel/syscalls/syscall_n64.tbl'),
]

def header(o):
    print('/// An enum of all syscalls', file=o)
    print('#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]', file=o)
    print('#[allow(non_camel_case_types)]', file=o)
    print('#[non_exhaustive]', file=o)
    print('pub enum Syscall {', file=o)

def footer(o):
    print('}', file=o)

def convert_tbl(out, f):
    with open(out, 'w') as o:
        header(o)
        for l in f:
            m = re.match('^(\\d+)\\s+(\\S+)\\s+(\\S+)(?:\\s+(\\S+))?', l)
            if m:
                nr, abi, name, entrypoint = m.groups()
                if name in {'break'}:
                    name = '_' + name
                if abi in abis:
                    print(name + ' = ' + nr + ',', file=o)
        footer(o)

UNISTD_MAP64 = {
    'fcntl': 'fcntl',
    'statfs': 'statfs',
    'fstatfs': 'fstatfs',
    'truncate': 'truncate',
    'ftruncate': 'ftruncate',
    'lseek': 'lseek',
    'sendfile': 'sendfile',
    'fstatat': 'newfstatat',
    'fstat': 'fstat',
    'mmap': 'mmap',
    'fadvise64': 'fadvise64',
    'stat': 'stat',
    'lstat': 'lstat',
}

UNISTD_MAP32 = {
    'fcntl':'fcntl64',
    'statfs':'statfs64',
    'fstatfs':'fstatfs64',
    'truncate':'truncate64',
    'ftruncate':'ftruncate64',
    'lseek':'llseek',
    'sendfile':'sendfile64',
    'fstatat':'fstatat64',
    'fstat':'fstat64',
    'mmap':'mmap2',
    'fadvise64':'fadvise64_64',
    'stat':'stat64',
    'lstat':'lstat64',
}

UNISTD_SOURCES = [
    ('b64/riscv64', UNISTD_MAP64),
    ('b64/aarch64', UNISTD_MAP64),
]

def convert_unistd(out, unistd_map, f):
    with open(out, 'w') as o:
        header(o)
        uniq = set()
        for l in f:
            m = re.match('#define __NR(3264)?_([^ ]+)\\s+([0-9]+)$', l)
            if m:
                name = m[2]
                nr = m[3]
                if name in {'syscalls', 'arch_specific_syscall'}:
                    continue
                if m[1]:
                    name = unistd_map[name]
                if nr in uniq:
                    continue
                print(name + ' = ' + nr + ',', file=o)
                uniq.add(nr)
        footer(o)

def setup_path_for_name(name):
    out = pathlib.Path('src/' + name + '.rs')
    out.parents[0].mkdir(parents=True, exist_ok=True)
    return out

for (name, abis, path) in SOURCES:
    with open(BASE + '/' + path) as f:
        out = setup_path_for_name(name)
        convert_tbl(out, f)

for (name, unistd_map) in UNISTD_SOURCES:
    with open(BASE + '/include/uapi/asm-generic/unistd.h') as f:
        out = setup_path_for_name(name)
        convert_unistd(out, unistd_map, f)
