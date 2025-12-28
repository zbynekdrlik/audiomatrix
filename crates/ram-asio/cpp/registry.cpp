/**
 * AudioMatrix Virtual ASIO Driver - Windows Registry Integration
 *
 * Handles COM registration and ASIO driver registry entries.
 */

// INITGUID must be defined before including any headers that use DEFINE_GUID
// This causes DEFINE_GUID to actually define the GUID rather than just declare it
#include <initguid.h>
#include "virtual_asio.h"
#include <objbase.h>
#include <olectl.h>  // For SELFREG_E_CLASS
#include <string>

namespace {

const wchar_t DRIVER_NAME[] = L"AudioMatrix Virtual";
const wchar_t DRIVER_DESCRIPTION[] = L"AudioMatrix Virtual ASIO Driver";

// Get the path to this DLL
std::wstring getModulePath() {
    wchar_t path[MAX_PATH] = {0};
    HMODULE hModule = nullptr;

    GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        reinterpret_cast<LPCWSTR>(&getModulePath),
        &hModule
    );

    GetModuleFileNameW(hModule, path, MAX_PATH);
    return path;
}

// Convert GUID to string
std::wstring guidToString(const GUID& guid) {
    wchar_t buffer[64];
    swprintf_s(buffer, L"{%08X-%04X-%04X-%02X%02X-%02X%02X%02X%02X%02X%02X}",
        guid.Data1, guid.Data2, guid.Data3,
        guid.Data4[0], guid.Data4[1], guid.Data4[2], guid.Data4[3],
        guid.Data4[4], guid.Data4[5], guid.Data4[6], guid.Data4[7]);
    return buffer;
}

// Create registry key and set value
bool setRegistryValue(HKEY hKeyRoot, const wchar_t* subKey,
                      const wchar_t* valueName, const wchar_t* value) {
    HKEY hKey;
    LONG result = RegCreateKeyExW(hKeyRoot, subKey, 0, nullptr,
                                  REG_OPTION_NON_VOLATILE, KEY_WRITE, nullptr,
                                  &hKey, nullptr);
    if (result != ERROR_SUCCESS) {
        return false;
    }

    result = RegSetValueExW(hKey, valueName, 0, REG_SZ,
                           reinterpret_cast<const BYTE*>(value),
                           static_cast<DWORD>((wcslen(value) + 1) * sizeof(wchar_t)));

    RegCloseKey(hKey);
    return result == ERROR_SUCCESS;
}

// Delete registry key recursively
bool deleteRegistryKey(HKEY hKeyRoot, const wchar_t* subKey) {
    return RegDeleteTreeW(hKeyRoot, subKey) == ERROR_SUCCESS;
}

} // anonymous namespace

extern "C" {

/**
 * Register the ASIO driver in the Windows registry.
 *
 * Creates entries in:
 * - HKLM\SOFTWARE\ASIO\AudioMatrix Virtual
 * - HKLM\SOFTWARE\Classes\CLSID\{...}
 */
STDAPI DllRegisterServer() {
    std::wstring clsidStr = guidToString(CLSID_AudioMatrixASIO);
    std::wstring modulePath = getModulePath();

    // Register ASIO driver
    // HKLM\SOFTWARE\ASIO\AudioMatrix Virtual
    std::wstring asioKey = L"SOFTWARE\\ASIO\\";
    asioKey += DRIVER_NAME;

    if (!setRegistryValue(HKEY_LOCAL_MACHINE, asioKey.c_str(), L"CLSID", clsidStr.c_str())) {
        return SELFREG_E_CLASS;
    }

    if (!setRegistryValue(HKEY_LOCAL_MACHINE, asioKey.c_str(), L"Description", DRIVER_DESCRIPTION)) {
        return SELFREG_E_CLASS;
    }

    // Register COM class
    // HKLM\SOFTWARE\Classes\CLSID\{...}
    std::wstring clsidKey = L"SOFTWARE\\Classes\\CLSID\\";
    clsidKey += clsidStr;

    if (!setRegistryValue(HKEY_LOCAL_MACHINE, clsidKey.c_str(), nullptr, DRIVER_NAME)) {
        return SELFREG_E_CLASS;
    }

    // InprocServer32
    std::wstring inprocKey = clsidKey + L"\\InprocServer32";
    if (!setRegistryValue(HKEY_LOCAL_MACHINE, inprocKey.c_str(), nullptr, modulePath.c_str())) {
        return SELFREG_E_CLASS;
    }

    if (!setRegistryValue(HKEY_LOCAL_MACHINE, inprocKey.c_str(), L"ThreadingModel", L"Apartment")) {
        return SELFREG_E_CLASS;
    }

    return S_OK;
}

/**
 * Unregister the ASIO driver from the Windows registry.
 */
STDAPI DllUnregisterServer() {
    std::wstring clsidStr = guidToString(CLSID_AudioMatrixASIO);

    // Remove ASIO driver entry
    std::wstring asioKey = L"SOFTWARE\\ASIO\\";
    asioKey += DRIVER_NAME;
    deleteRegistryKey(HKEY_LOCAL_MACHINE, asioKey.c_str());

    // Remove COM class entry
    std::wstring clsidKey = L"SOFTWARE\\Classes\\CLSID\\";
    clsidKey += clsidStr;
    deleteRegistryKey(HKEY_LOCAL_MACHINE, clsidKey.c_str());

    return S_OK;
}

/**
 * COM DLL entry point for class factory.
 *
 * ASIO is unusual in that it treats the CLSID as an IID and expects
 * the driver instance directly, not a class factory.
 */
STDAPI DllGetClassObject(REFCLSID rclsid, REFIID riid, LPVOID* ppv) {
    if (!ppv) {
        return E_POINTER;
    }

    *ppv = nullptr;

    // Check if requesting our CLSID
    if (!IsEqualCLSID(rclsid, CLSID_AudioMatrixASIO)) {
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    // ASIO hosts typically request IID_IUnknown or use the CLSID as the IID
    // We return the driver instance directly (ASIO doesn't use COM class factories)
    if (IsEqualIID(riid, IID_IUnknown) || IsEqualIID(riid, CLSID_AudioMatrixASIO)) {
        IASIO* driver = audiomatrix::CreateAudioMatrixDriver();
        if (driver) {
            *ppv = static_cast<void*>(driver);
            return S_OK;
        }
        return E_OUTOFMEMORY;
    }

    return E_NOINTERFACE;
}

/**
 * Check if DLL can be unloaded.
 */
STDAPI DllCanUnloadNow() {
    // Check if driver instance exists and has references
    if (audiomatrix::g_driverInstance != nullptr) {
        return S_FALSE; // Don't unload while driver exists
    }
    if (audiomatrix::g_serverLockCount > 0) {
        return S_FALSE;
    }
    return S_OK;
}

} // extern "C"
