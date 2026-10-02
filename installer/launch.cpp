// Installer-only launcher: ask the interactive Explorer to start the tray
// with its normal user token, rather than inheriting the installer's elevation.
#include <windows.h>
#include <exdisp.h>
#include <servprov.h>
#include <shldisp.h>
#include <shlguid.h>
#include <shobjidl.h>
#include <shellapi.h>

template<class T> struct ComPtr {
    T* value = nullptr;
    ~ComPtr() { if (value) value->Release(); }
    T* operator->() const { return value; }
    T** address() { return &value; }
};

static HRESULT launch(const wchar_t* path) {
    ComPtr<IShellWindows> windows;
    HRESULT result = CoCreateInstance(CLSID_ShellWindows, nullptr,
        CLSCTX_LOCAL_SERVER, IID_PPV_ARGS(windows.address()));
    if (FAILED(result)) return result;

    VARIANT empty;
    VariantInit(&empty);
    long desktop = 0;
    ComPtr<IDispatch> dispatch;
    result = windows->FindWindowSW(&empty, &empty, SWC_DESKTOP,
        &desktop, SWFO_NEEDDISPATCH, dispatch.address());
    if (result != S_OK || !dispatch.value) return E_FAIL;

    ComPtr<IServiceProvider> services;
    result = dispatch->QueryInterface(IID_PPV_ARGS(services.address()));
    if (FAILED(result)) return result;
    ComPtr<IShellBrowser> browser;
    result = services->QueryService(SID_STopLevelBrowser,
        IID_PPV_ARGS(browser.address()));
    if (FAILED(result)) return result;
    ComPtr<IShellView> view;
    result = browser->QueryActiveShellView(view.address());
    if (FAILED(result)) return result;
    ComPtr<IDispatch> background;
    result = view->GetItemObject(SVGIO_BACKGROUND,
        IID_PPV_ARGS(background.address()));
    if (FAILED(result)) return result;
    ComPtr<IShellFolderViewDual> folder;
    result = background->QueryInterface(IID_PPV_ARGS(folder.address()));
    if (FAILED(result)) return result;
    ComPtr<IDispatch> application;
    result = folder->get_Application(application.address());
    if (FAILED(result)) return result;
    ComPtr<IShellDispatch2> shell;
    result = application->QueryInterface(IID_PPV_ARGS(shell.address()));
    if (FAILED(result)) return result;

    BSTR command = SysAllocString(path);
    if (!command) return E_OUTOFMEMORY;
    VARIANT show;
    VariantInit(&show);
    show.vt = VT_I4;
    show.lVal = SW_SHOWNORMAL;
    result = shell->ShellExecute(command, empty, empty, empty, show);
    SysFreeString(command);
    return result;
}

extern "C" void mainCRTStartup() {
    int count = 0;
    wchar_t** arguments = CommandLineToArgvW(GetCommandLineW(), &count);
    HRESULT result = E_INVALIDARG;
    if (arguments && count == 2) {
        result = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
        if (SUCCEEDED(result)) {
            result = launch(arguments[1]);
            CoUninitialize();
        }
    }
    if (arguments) LocalFree(arguments);
    ExitProcess(SUCCEEDED(result) ? 0 : 1);
}
