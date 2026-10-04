"""Read-only process audit. Nothing is injected, trimmed, or changed in the target."""
import argparse
import ctypes as c
from ctypes import wintypes as w
import json
from pathlib import Path
import time


class Counters(c.Structure):
    _fields_ = [("cb", w.DWORD), ("PageFaultCount", w.DWORD)] + [
        (name, c.c_size_t) for name in (
            "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
            "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
            "PagefileUsage", "PeakPagefileUsage", "PrivateUsage", "PrivateWorkingSetSize"
        )] + [("SharedCommitUsage", c.c_uint64)]


class IoCounters(c.Structure):
    _fields_ = [(name, c.c_uint64) for name in (
        "ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
        "ReadTransferCount", "WriteTransferCount", "OtherTransferCount"
    )]


class ModuleInfo(c.Structure):
    _fields_ = [("base", c.c_void_p), ("size", w.DWORD), ("entry", c.c_void_p)]


k = c.WinDLL("kernel32", use_last_error=True)
p = c.WinDLL("psapi", use_last_error=True)
u = c.WinDLL("user32", use_last_error=True)
k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
k.OpenProcess.restype = w.HANDLE
k.CloseHandle.argtypes = [w.HANDLE]
k.GetProcessTimes.argtypes = [w.HANDLE] + [c.POINTER(w.FILETIME)] * 4
k.GetProcessIoCounters.argtypes = [w.HANDLE, c.POINTER(IoCounters)]
k.GetProcessHandleCount.argtypes = [w.HANDLE, c.POINTER(w.DWORD)]
u.GetGuiResources.argtypes = [w.HANDLE, w.DWORD]
u.GetGuiResources.restype = w.DWORD
p.GetProcessMemoryInfo.argtypes = [w.HANDLE, c.POINTER(Counters), w.DWORD]
p.QueryWorkingSet.argtypes = [w.HANDLE, c.c_void_p, w.DWORD]
p.EnumProcessModulesEx.argtypes = [w.HANDLE, c.POINTER(w.HMODULE), w.DWORD, c.POINTER(w.DWORD), w.DWORD]
p.GetModuleInformation.argtypes = [w.HANDLE, w.HMODULE, c.POINTER(ModuleInfo), w.DWORD]
p.GetModuleFileNameExW.argtypes = [w.HANDLE, w.HMODULE, w.LPWSTR, w.DWORD]


def checked(result):
    if not result:
        raise c.WinError(c.get_last_error())


def sample(handle):
    counters = Counters()
    counters.cb = c.sizeof(counters)
    checked(p.GetProcessMemoryInfo(handle, c.byref(counters), c.sizeof(counters)))
    times = [w.FILETIME() for _ in range(4)]
    checked(k.GetProcessTimes(handle, *[c.byref(t) for t in times]))
    ticks = sum((t.dwHighDateTime << 32) | t.dwLowDateTime for t in times[2:])
    io = IoCounters()
    checked(k.GetProcessIoCounters(handle, c.byref(io)))
    handles = w.DWORD()
    checked(k.GetProcessHandleCount(handle, c.byref(handles)))
    return {"time": time.time(), "working_set": counters.WorkingSetSize,
            "private_commit": counters.PrivateUsage,
            "private_working_set": counters.PrivateWorkingSetSize,
            "shared_working_set": counters.WorkingSetSize - counters.PrivateWorkingSetSize,
            "page_faults": counters.PageFaultCount, "cpu_ms": ticks / 10000,
            "read_bytes": io.ReadTransferCount, "write_bytes": io.WriteTransferCount,
            "gdi_objects": u.GetGuiResources(handle, 0),
            "user_objects": u.GetGuiResources(handle, 1), "handle_count": handles.value}


def modules(handle):
    buffer = (w.HMODULE * 256)()
    needed = w.DWORD()
    checked(p.EnumProcessModulesEx(handle, buffer, c.sizeof(buffer), c.byref(needed), 3))
    if needed.value > c.sizeof(buffer):
        raise RuntimeError("Module buffer too small")
    result = []
    for module in buffer[:needed.value // c.sizeof(w.HMODULE)]:
        name = c.create_unicode_buffer(32768)
        info = ModuleInfo()
        checked(p.GetModuleFileNameExW(handle, module, name, len(name)))
        checked(p.GetModuleInformation(handle, module, c.byref(info), c.sizeof(info)))
        result.append({"name": Path(name.value).name, "base": info.base, "mapped_bytes": info.size,
                       "resident_bytes": 0, "private_resident_bytes": 0})
    pages = (c.c_size_t * 32769)()
    checked(p.QueryWorkingSet(handle, pages, c.sizeof(pages)))
    if pages[0] >= len(pages):
        raise RuntimeError("Working-set buffer too small")
    unassigned = 0
    for flags in pages[1:pages[0] + 1]:
        address = (flags >> 12) << 12
        module = next((m for m in result if m["base"] <= address < m["base"] + m["mapped_bytes"]), None)
        if module:
            module["resident_bytes"] += 4096
            if not flags & 0x100:
                module["private_resident_bytes"] += 4096
        else:
            unassigned += 4096
    return sorted(result, key=lambda m: m["resident_bytes"], reverse=True), unassigned


parser = argparse.ArgumentParser()
parser.add_argument("pid", type=int)
parser.add_argument("--seconds", type=float, default=30)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
handle = k.OpenProcess(0x410, False, args.pid)  # QUERY_INFORMATION | VM_READ
checked(handle)
try:
    snapshots = [sample(handle)]
    deadline = time.monotonic() + args.seconds
    while time.monotonic() < deadline:
        time.sleep(min(5, max(0, deadline - time.monotonic())))
        snapshots.append(sample(handle))
    dlls, other = modules(handle)
    first, last = snapshots[0], snapshots[-1]
    report = {"pid": args.pid, "seconds": last["time"] - first["time"], "samples": snapshots,
              "cpu_delta_ms": last["cpu_ms"] - first["cpu_ms"],
              "read_bytes_delta": last["read_bytes"] - first["read_bytes"],
              "write_bytes_delta": last["write_bytes"] - first["write_bytes"],
              "page_fault_delta": last["page_faults"] - first["page_faults"],
              "modules": dlls, "heap_stack_other_resident_bytes": other}
    args.output.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps({key: report[key] for key in ("pid", "seconds", "cpu_delta_ms", "read_bytes_delta", "write_bytes_delta", "page_fault_delta")} | {"last_sample": last}, indent=2))
finally:
    k.CloseHandle(handle)
