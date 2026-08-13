#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <bcrypt.h>
#include <tlhelp32.h>

#include <array>
#include <cstdint>
#include <cstdio>
#include <cwchar>
#include <string>

#pragma comment(lib, "bcrypt.lib")

namespace {
constexpr std::uint32_t kParameterSchemaV1 = 1;
constexpr std::size_t kMaximumPathCharacters = 32768;
constexpr std::size_t kChannelCharacters = 128;
constexpr DWORD kOutputBytes = 1048576;
constexpr DWORD kWaitMilliseconds = 30000;
constexpr wchar_t kChannelPrefix[] = L"Local\\mtgkernel_mtgo_visible_v1_";
constexpr wchar_t kOnlyAdmittedTargetFileName[] =
    L"synthetic_managed_host_v1.exe";

struct VisibleDuelBootstrapParametersV1 {
  std::uint32_t schema_version;
  std::uint32_t structure_bytes;
  wchar_t producer_path[kMaximumPathCharacters];
  wchar_t channel_name[kChannelCharacters];
};

struct HandleV1 {
  HANDLE value = nullptr;
  ~HandleV1() {
    if (value != nullptr && value != INVALID_HANDLE_VALUE) {
      CloseHandle(value);
    }
  }
};

bool IsAbsoluteExistingFileV1(const wchar_t* path) {
  if (path == nullptr || path[0] == L'\0') {
    return false;
  }
  wchar_t full[kMaximumPathCharacters] = {};
  DWORD length = GetFullPathNameW(path, static_cast<DWORD>(std::size(full)),
                                  full, nullptr);
  if (length == 0 || length >= std::size(full) ||
      _wcsicmp(path, full) != 0) {
    return false;
  }
  DWORD attributes = GetFileAttributesW(path);
  return attributes != INVALID_FILE_ATTRIBUTES &&
         (attributes & FILE_ATTRIBUTE_DIRECTORY) == 0 &&
         (attributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0;
}

bool ExactCopyV1(wchar_t* destination, std::size_t capacity,
                 const wchar_t* source) {
  if (destination == nullptr || source == nullptr || capacity == 0) {
    return false;
  }
  std::size_t length = wcslen(source);
  if (length == 0 || length >= capacity) {
    return false;
  }
  memcpy(destination, source, (length + 1) * sizeof(wchar_t));
  return true;
}

bool IsExactSyntheticTargetV1(HANDLE process) {
  wchar_t image_path[kMaximumPathCharacters] = {};
  DWORD length = static_cast<DWORD>(std::size(image_path));
  if (!QueryFullProcessImageNameW(process, 0, image_path, &length) ||
      length == 0 || length >= std::size(image_path)) {
    return false;
  }
  const wchar_t* file_name = wcsrchr(image_path, L'\\');
  file_name = file_name == nullptr ? image_path : file_name + 1;
  if (wcscmp(file_name, kOnlyAdmittedTargetFileName) != 0) {
    return false;
  }

  USHORT process_machine = IMAGE_FILE_MACHINE_UNKNOWN;
  USHORT native_machine = IMAGE_FILE_MACHINE_UNKNOWN;
  if (!IsWow64Process2(process, &process_machine, &native_machine)) {
    return false;
  }
  return process_machine == IMAGE_FILE_MACHINE_UNKNOWN &&
         native_machine == IMAGE_FILE_MACHINE_AMD64;
}

bool MakeChannelNameV1(std::wstring& output) {
  std::array<unsigned char, 32> random_bytes{};
  if (BCryptGenRandom(nullptr, random_bytes.data(),
                      static_cast<ULONG>(random_bytes.size()),
                      BCRYPT_USE_SYSTEM_PREFERRED_RNG) != 0) {
    return false;
  }
  static constexpr wchar_t hex[] = L"0123456789abcdef";
  output.assign(kChannelPrefix);
  for (unsigned char byte : random_bytes) {
    output.push_back(hex[byte >> 4]);
    output.push_back(hex[byte & 0x0f]);
  }
  return output.size() < kChannelCharacters;
}

std::uintptr_t RemoteModuleBaseV1(DWORD process_id,
                                  const wchar_t* exact_path) {
  HandleV1 snapshot{CreateToolhelp32Snapshot(
      TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, process_id)};
  if (snapshot.value == INVALID_HANDLE_VALUE) {
    return 0;
  }
  MODULEENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  if (!Module32FirstW(snapshot.value, &entry)) {
    return 0;
  }
  do {
    if (_wcsicmp(entry.szExePath, exact_path) == 0) {
      return reinterpret_cast<std::uintptr_t>(entry.modBaseAddr);
    }
  } while (Module32NextW(snapshot.value, &entry));
  return 0;
}

std::uintptr_t RemoteSystemProcedureV1(DWORD process_id,
                                       const wchar_t* module_name,
                                       const char* procedure_name) {
  HMODULE local_module = GetModuleHandleW(module_name);
  FARPROC local_procedure =
      local_module == nullptr ? nullptr
                              : GetProcAddress(local_module, procedure_name);
  if (local_module == nullptr || local_procedure == nullptr) {
    return 0;
  }

  HandleV1 snapshot{CreateToolhelp32Snapshot(
      TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, process_id)};
  if (snapshot.value == INVALID_HANDLE_VALUE) {
    return 0;
  }
  MODULEENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  if (!Module32FirstW(snapshot.value, &entry)) {
    return 0;
  }
  do {
    if (_wcsicmp(entry.szModule, module_name) == 0) {
      auto offset = reinterpret_cast<std::uintptr_t>(local_procedure) -
                    reinterpret_cast<std::uintptr_t>(local_module);
      return reinterpret_cast<std::uintptr_t>(entry.modBaseAddr) + offset;
    }
  } while (Module32NextW(snapshot.value, &entry));
  return 0;
}

bool RunRemoteThreadV1(HANDLE process, std::uintptr_t start, void* parameter,
                       DWORD& exit_code) {
  HandleV1 thread{CreateRemoteThread(
      process, nullptr, 0,
      reinterpret_cast<LPTHREAD_START_ROUTINE>(start), parameter, 0, nullptr)};
  if (thread.value == nullptr ||
      WaitForSingleObject(thread.value, kWaitMilliseconds) != WAIT_OBJECT_0) {
    return false;
  }
  return GetExitCodeThread(thread.value, &exit_code) != FALSE;
}

bool AllowedResultV1(const char* bytes, DWORD length) {
  static constexpr const char* allowed[] = {
      "{\"result_kind\":\"abstained\",\"reason\":\"duel_surface_unavailable\"}",
      "{\"result_kind\":\"abstained\",\"reason\":\"surface_shape_mismatch\"}",
      "{\"result_kind\":\"abstained\",\"reason\":\"projection_incomplete\"}",
      "{\"result_kind\":\"abstained\",\"reason\":\"output_validation_failed\"}",
  };
  for (const char* candidate : allowed) {
    std::size_t candidate_length = strlen(candidate);
    if (candidate_length == length &&
        memcmp(candidate, bytes, length) == 0) {
      return true;
    }
  }
  return false;
}

int FailV1(const char* code) {
  std::fprintf(stderr, "mtgo_visible_duel_broker_v1:%s\n", code);
  return 1;
}
}  // namespace

int wmain(int argc, wchar_t** argv) {
  if (argc != 7 || wcscmp(argv[1], L"--pid") != 0 ||
      wcscmp(argv[3], L"--bootstrap") != 0 ||
      wcscmp(argv[5], L"--producer") != 0) {
    return FailV1("arguments");
  }
  wchar_t* pid_end = nullptr;
  unsigned long parsed_pid = wcstoul(argv[2], &pid_end, 10);
  if (parsed_pid == 0 || pid_end == nullptr || *pid_end != L'\0' ||
      !IsAbsoluteExistingFileV1(argv[4]) ||
      !IsAbsoluteExistingFileV1(argv[6])) {
    return FailV1("input_validation");
  }
  DWORD process_id = static_cast<DWORD>(parsed_pid);

  std::wstring channel_name;
  if (!MakeChannelNameV1(channel_name)) {
    return FailV1("channel_name");
  }
  HandleV1 mapping{CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr,
                                      PAGE_READWRITE, 0, kOutputBytes,
                                      channel_name.c_str())};
  if (mapping.value == nullptr || GetLastError() == ERROR_ALREADY_EXISTS) {
    return FailV1("channel_create");
  }
  void* channel_view =
      MapViewOfFile(mapping.value, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0,
                    kOutputBytes);
  if (channel_view == nullptr) {
    return FailV1("channel_map");
  }
  SecureZeroMemory(channel_view, kOutputBytes);

  HandleV1 process{OpenProcess(PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION |
                                   PROCESS_QUERY_LIMITED_INFORMATION |
                                   PROCESS_VM_OPERATION | PROCESS_VM_WRITE |
                                   SYNCHRONIZE,
                               FALSE, process_id)};
  if (process.value == nullptr) {
    UnmapViewOfFile(channel_view);
    return FailV1("process_open");
  }
  if (!IsExactSyntheticTargetV1(process.value)) {
    UnmapViewOfFile(channel_view);
    return FailV1("target_not_synthetic_host");
  }

  std::size_t bootstrap_path_bytes =
      (wcslen(argv[4]) + 1) * sizeof(wchar_t);
  void* remote_bootstrap_path =
      VirtualAllocEx(process.value, nullptr, bootstrap_path_bytes,
                     MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  SIZE_T written = 0;
  if (remote_bootstrap_path == nullptr ||
      !WriteProcessMemory(process.value, remote_bootstrap_path, argv[4],
                          bootstrap_path_bytes, &written) ||
      written != bootstrap_path_bytes) {
    UnmapViewOfFile(channel_view);
    return FailV1("bootstrap_write");
  }
  std::uintptr_t load_library =
      RemoteSystemProcedureV1(process_id, L"KernelBase.dll", "LoadLibraryW");
  DWORD thread_status = 0;
  if (load_library == 0 ||
      !RunRemoteThreadV1(process.value, load_library, remote_bootstrap_path,
                         thread_status) ||
      thread_status == 0) {
    VirtualFreeEx(process.value, remote_bootstrap_path, 0, MEM_RELEASE);
    UnmapViewOfFile(channel_view);
    return FailV1("bootstrap_load");
  }
  VirtualFreeEx(process.value, remote_bootstrap_path, 0, MEM_RELEASE);

  std::uintptr_t remote_bootstrap = RemoteModuleBaseV1(process_id, argv[4]);
  HMODULE local_bootstrap = LoadLibraryExW(
      argv[4], nullptr, DONT_RESOLVE_DLL_REFERENCES);
  FARPROC local_entry = local_bootstrap == nullptr
                            ? nullptr
                            : GetProcAddress(local_bootstrap,
                                             "RunVisibleDuelProducerV1");
  if (remote_bootstrap == 0 || local_bootstrap == nullptr ||
      local_entry == nullptr) {
    if (local_bootstrap != nullptr) {
      FreeLibrary(local_bootstrap);
    }
    UnmapViewOfFile(channel_view);
    return FailV1("bootstrap_entry");
  }
  std::uintptr_t entry_offset =
      reinterpret_cast<std::uintptr_t>(local_entry) -
      reinterpret_cast<std::uintptr_t>(local_bootstrap);
  FreeLibrary(local_bootstrap);

  VisibleDuelBootstrapParametersV1 parameters{};
  parameters.schema_version = kParameterSchemaV1;
  parameters.structure_bytes = sizeof(parameters);
  if (!ExactCopyV1(parameters.producer_path,
                   std::size(parameters.producer_path), argv[6]) ||
      !ExactCopyV1(parameters.channel_name,
                   std::size(parameters.channel_name), channel_name.c_str())) {
    UnmapViewOfFile(channel_view);
    return FailV1("parameter_copy");
  }
  void* remote_parameters =
      VirtualAllocEx(process.value, nullptr, sizeof(parameters),
                     MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  if (remote_parameters == nullptr ||
      !WriteProcessMemory(process.value, remote_parameters, &parameters,
                          sizeof(parameters), &written) ||
      written != sizeof(parameters)) {
    UnmapViewOfFile(channel_view);
    return FailV1("parameter_write");
  }
  SecureZeroMemory(&parameters, sizeof(parameters));

  bool invoked = RunRemoteThreadV1(process.value,
                                   remote_bootstrap + entry_offset,
                                   remote_parameters, thread_status);
  std::array<unsigned char, sizeof(VisibleDuelBootstrapParametersV1)> zeros{};
  WriteProcessMemory(process.value, remote_parameters, zeros.data(),
                     zeros.size(), &written);
  VirtualFreeEx(process.value, remote_parameters, 0, MEM_RELEASE);
  if (!invoked || thread_status != 0) {
    SecureZeroMemory(channel_view, kOutputBytes);
    UnmapViewOfFile(channel_view);
    return FailV1("producer_invoke");
  }

  MemoryBarrier();
  const auto* header = static_cast<const std::uint32_t*>(channel_view);
  DWORD length = header[0];
  DWORD schema = header[1];
  const char* payload = static_cast<const char*>(channel_view) + 8;
  if (schema != 1 || length == 0 || length > kOutputBytes - 8 ||
      !AllowedResultV1(payload, length)) {
    SecureZeroMemory(channel_view, kOutputBytes);
    UnmapViewOfFile(channel_view);
    return FailV1("output_validation");
  }
  std::fwrite(payload, 1, length, stdout);
  std::fputc('\n', stdout);
  SecureZeroMemory(channel_view, kOutputBytes);
  UnmapViewOfFile(channel_view);
  return 0;
}
