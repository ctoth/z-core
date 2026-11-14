"""Build and run the authored C ABI consumer in C/C++, static/shared modes.

Run explicitly: python crates/z180-c/tests/consumer.py
Only this binding's library and consumers are built; no other tests are run.
"""

import os
from pathlib import Path
import shutil
import subprocess


def run(*args, **kwargs):
    print("+", " ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), check=True, **kwargs)


def main():
    root = Path(__file__).resolve().parents[3]
    crate = root / "crates/z180-c"
    target = root / "target/c-abi-consumer"
    run("cargo", "build", "-p", "z180-c", "--target-dir", target, cwd=root)
    lib = target / "debug"
    compiler = shutil.which("clang")
    if not compiler:
        raise RuntimeError("clang is required for the C/C++ ABI consumer test")
    windows = os.name == "nt"
    env = dict(os.environ)
    env["PATH"] = str(lib) + os.pathsep + env.get("PATH", "")
    env["LD_LIBRARY_PATH"] = str(lib) + os.pathsep + env.get("LD_LIBRARY_PATH", "")
    for language in ("c", "c++"):
        for linkage in ("static", "shared"):
            exe = lib / (f"consumer-{language.replace('+', 'p')}-{linkage}" + (".exe" if windows else ""))
            flags = ["-x", language, "-std=c99" if language == "c" else "-std=c++11", "-Wall", "-Wextra", "-Werror"]
            if linkage == "static":
                flags.append("-DZ180_STATIC")
            if windows:
                libraries = [lib / ("z180.lib" if linkage == "static" else "z180.dll.lib")]
                if linkage == "static":
                    libraries += ["-lws2_32", "-luserenv", "-lntdll", "-lbcrypt", "-ladvapi32", "-lsynchronization"]
            else:
                libraries = [lib / ("libz180.a" if linkage == "static" else "libz180.so"), "-lpthread", "-ldl", "-lm"]
            run(compiler, *flags, "-I", crate / "include", crate / "tests/consumer.c", "-x", "none", *libraries, "-o", exe, cwd=root)
            run(exe, env=env, cwd=root)
    # Compile and execute the documented minimal board example too.
    exe = lib / ("board.exe" if windows else "board")
    shared = lib / ("z180.dll.lib" if windows else "libz180.so")
    run(compiler, "-std=c99", "-Wall", "-Wextra", "-Werror", "-I", crate / "include", crate / "examples/board.c", shared, "-o", exe, cwd=root)
    run(exe, env=env, cwd=root)


if __name__ == "__main__":
    main()
