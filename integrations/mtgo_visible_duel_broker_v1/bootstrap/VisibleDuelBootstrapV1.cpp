#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <metahost.h>
#include <mscoree.h>

#include <cstddef>
#include <cstdint>

#pragma comment(lib, "mscoree.lib")

namespace {
constexpr std::uint32_t kParameterSchemaV1 = 1;
constexpr wchar_t kRuntimeVersion[] = L"v4.0.30319";
constexpr wchar_t kProducerType[] =
    L"MtgKernel.Mtgo.VisibleDuelProducer.V1.VisibleDuelProducerV1";
constexpr std::size_t kMaximumPathCharacters = 32768;
constexpr std::size_t kChannelCharacters = 128;

struct VisibleDuelBootstrapParametersV1 {
  std::uint32_t schema_version;
  std::uint32_t structure_bytes;
  wchar_t producer_path[kMaximumPathCharacters];
  wchar_t channel_name[kChannelCharacters];
  wchar_t producer_method[kChannelCharacters];
};

bool IsTerminatedWithinV1(const wchar_t* value, std::size_t capacity) {
  if (value == nullptr || capacity == 0) {
    return false;
  }
  for (std::size_t index = 0; index < capacity; ++index) {
    if (value[index] == L'\0') {
      return index > 0;
    }
  }
  return false;
}
}  // namespace

extern "C" __declspec(dllexport) DWORD WINAPI RunVisibleDuelProducerV1(
    void* opaque_parameters) {
  if (opaque_parameters == nullptr) {
    return 10;
  }
  auto* parameters =
      static_cast<VisibleDuelBootstrapParametersV1*>(opaque_parameters);
  if (parameters->schema_version != kParameterSchemaV1 ||
      parameters->structure_bytes != sizeof(VisibleDuelBootstrapParametersV1) ||
      !IsTerminatedWithinV1(parameters->producer_path,
                            kMaximumPathCharacters) ||
      !IsTerminatedWithinV1(parameters->channel_name, kChannelCharacters) ||
      !IsTerminatedWithinV1(parameters->producer_method,
                            kChannelCharacters)) {
    return 11;
  }

  ICLRMetaHost* meta_host = nullptr;
  ICLRRuntimeInfo* runtime_info = nullptr;
  ICLRRuntimeHost* runtime_host = nullptr;
  DWORD managed_status = 0;

  HRESULT result = CLRCreateInstance(
      CLSID_CLRMetaHost, IID_ICLRMetaHost,
      reinterpret_cast<void**>(&meta_host));
  if (SUCCEEDED(result)) {
    result = meta_host->GetRuntime(
        kRuntimeVersion, IID_ICLRRuntimeInfo,
        reinterpret_cast<void**>(&runtime_info));
  }
  BOOL loaded = FALSE;
  if (SUCCEEDED(result)) {
    result = runtime_info->IsLoaded(GetCurrentProcess(), &loaded);
    if (SUCCEEDED(result) && loaded == FALSE) {
      result = E_FAIL;
    }
  }
  if (SUCCEEDED(result)) {
    result = runtime_info->GetInterface(
        CLSID_CLRRuntimeHost, IID_ICLRRuntimeHost,
        reinterpret_cast<void**>(&runtime_host));
  }
  if (SUCCEEDED(result)) {
    result = runtime_host->ExecuteInDefaultAppDomain(
        parameters->producer_path, kProducerType, parameters->producer_method,
        parameters->channel_name, &managed_status);
  }

  if (runtime_host != nullptr) {
    runtime_host->Release();
  }
  if (runtime_info != nullptr) {
    runtime_info->Release();
  }
  if (meta_host != nullptr) {
    meta_host->Release();
  }

  if (FAILED(result)) {
    return 12;
  }
  return managed_status == 0 ? 0 : 13;
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, void*) {
  if (reason == DLL_PROCESS_ATTACH) {
    DisableThreadLibraryCalls(instance);
  }
  return TRUE;
}
