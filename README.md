**The primary purpose of this project is learning Rust**

# Parsing of BOM (Bill Of Material) files used by Apple

BOM is a file format that originated at NeXT Computer. It is still used by Apple today in MacOS. The primary use is to list the files, their permissions and other metadata in the installer packages (.pkg).

It's a rather complicated file format. There are a few metadata sections. One of these is a list of so called "variables" which are named (eg. Paths). These point to the section of the file that contains something called a BOM tree which in turn contains data related to the variable. In the case of an installer BOM file the tree contains a list of path components, which you then need to put together to form a list of files in the installer.

## This package

This package contains a library target and a binary. The library can be used to read the variables in the file and iterate over the corresponding trees. The binary mainly demonstrates its use but it is also a Rust equivalent to `lsbom` which is a closed-source binary provided by Apple to dump installer BOM files.

## The format

                                        ┌─────────────────────────────┐
                                        │          BOM tree           │
                                        └──────────────┬──────────────┘
                                                       │e
                                                       │n
                                                       │t
                                                       │r
                                                       │y
                                                       ▼
                                           ┌───────────────────────┐
                                           │    BOM tree entry     │
                                           └───────────┬───────────┘
                                                       │i
                                                       │n
                                                       │d
                                                       │i
                                                       │c
                                                       │e
                                                       │s
                                          ┌────────────┼────────────┐
                                          │            │            │
                                          │            │            │
                                          ▼            ▼            ▼
                                       ┌─────┐      ┌─────┐      ┌─────┐
                                       │entry│◀────▶│entry│◀────▶│entry│
                                       └──┬──┘      └─────┘      └─────┘
                                          │i
                                          │n
                                          │d
                                          │i
                                          │c
                                          │e
                                          │s
                             ┌────────────┼────────────┐
                             │            │            │
                             │            │            │
                             ▼            ▼            ▼
               ┏━━━━━━━━━━━━━━━━┓      ┌─────┐      ┌─────┐
               ┃     entry      ┃  ┌──▶│entry│◀────▶│entry│
               ┃ is_leaf: true  ┃◀─┘   └─────┘      └─────┘
               ┗━━━━━━━┳━━━━━━━━┛
                       │i
                       │n
                       │d
                       │i
                       │c
                       │e
                       │s
          ┌────────────┼────────────┐
          │            │            │
          │            │            │
          ▼            ▼            ▼
     ┌─────────┐  ┌─────────┐  ┌─────────┐
     │key:value│  │key:value│  │key:value│
     └─────────┘  └─────────┘  └─────────┘