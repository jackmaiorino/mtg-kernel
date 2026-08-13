#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <bcrypt.h>
#ifdef MTGO_LIVE_PINNED_V1
#include <softpub.h>
#include <wintrust.h>
#endif
#include <tlhelp32.h>

#include <array>
#include <cstdint>
#include <cstdio>
#include <cwchar>
#include <string>
#ifdef MTGO_LIVE_PINNED_V1
#include <vector>
#endif

#pragma comment(lib, "bcrypt.lib")
#ifdef MTGO_LIVE_PINNED_V1
#pragma comment(lib, "version.lib")
#pragma comment(lib, "wintrust.lib")
#endif

namespace {
constexpr std::uint32_t kParameterSchemaV1 = 1;
constexpr std::size_t kMaximumPathCharacters = 32768;
constexpr std::size_t kChannelCharacters = 128;
constexpr DWORD kOutputBytes = 1048576;
constexpr DWORD kWaitMilliseconds = 30000;
constexpr wchar_t kChannelPrefix[] = L"Local\\mtgkernel_mtgo_visible_v1_";
#ifndef MTGO_LIVE_PINNED_V1
constexpr wchar_t kOnlyAdmittedTargetFileName[] =
    L"synthetic_managed_host_v1.exe";
#else
constexpr wchar_t kOnlyAdmittedTargetFileName[] = L"MTGO.exe";
constexpr char kExpectedMtgoSha256[] =
    "bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92";
constexpr char kExpectedDuelSceneSha256[] =
    "72b99e1169f9f9445a510b2dae52f9212fb7300c2483b8bc8e02f5760f11904e";
constexpr char kExpectedCardSha256[] =
    "071338a98d845d5c8db6ebd2f3c847e38ad548f50ba11d2a36973438cdec2ea8";
constexpr char kExpectedBootstrapSha256[] =
    "9c62e801dbcb3fd21647ba1fc7837e1d49663b902530d5b2fb5a6197c9f465d4";
constexpr char kExpectedProducerSha256[] =
    "3898431b57758c803981a1b8c1a20c50a6cf04745ec2623735f2bd1cd6bae936";
#endif

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

bool IsNativeX64TargetWithExactFileNameV1(HANDLE process) {
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

#ifdef MTGO_LIVE_PINNED_V1
std::uint64_t FileTimeU64V1(const FILETIME& value) {
  return (static_cast<std::uint64_t>(value.dwHighDateTime) << 32) |
         value.dwLowDateTime;
}

bool ProcessStartTimeV1(HANDLE process, std::uint64_t& output) {
  FILETIME created{};
  FILETIME exited{};
  FILETIME kernel{};
  FILETIME user{};
  if (!GetProcessTimes(process, &created, &exited, &kernel, &user)) {
    return false;
  }
  output = FileTimeU64V1(created);
  return output != 0;
}

bool ExactlyOneMtgoProcessV1(DWORD expected_process_id) {
  HandleV1 snapshot{
      CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)};
  if (snapshot.value == INVALID_HANDLE_VALUE) {
    return false;
  }
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  if (!Process32FirstW(snapshot.value, &entry)) {
    return false;
  }
  DWORD count = 0;
  DWORD observed_process_id = 0;
  do {
    if (_wcsicmp(entry.szExeFile, kOnlyAdmittedTargetFileName) == 0) {
      ++count;
      observed_process_id = entry.th32ProcessID;
    }
  } while (Process32NextW(snapshot.value, &entry));
  return count == 1 && observed_process_id == expected_process_id;
}

bool Sha256FileV1(const wchar_t* path,
                  std::array<unsigned char, 32>& output) {
  HandleV1 file{CreateFileW(path, GENERIC_READ,
                            FILE_SHARE_READ | FILE_SHARE_WRITE |
                                FILE_SHARE_DELETE,
                            nullptr, OPEN_EXISTING,
                            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_SEQUENTIAL_SCAN,
                            nullptr)};
  if (file.value == INVALID_HANDLE_VALUE) {
    return false;
  }
  BCRYPT_ALG_HANDLE algorithm = nullptr;
  BCRYPT_HASH_HANDLE hash = nullptr;
  DWORD object_bytes = 0;
  DWORD returned = 0;
  std::vector<unsigned char> hash_object;
  std::array<unsigned char, 65536> buffer{};
  bool ok = BCryptOpenAlgorithmProvider(&algorithm, BCRYPT_SHA256_ALGORITHM,
                                        nullptr, 0) == 0;
  if (ok) {
    ok = BCryptGetProperty(algorithm, BCRYPT_OBJECT_LENGTH,
                           reinterpret_cast<unsigned char*>(&object_bytes),
                           sizeof(object_bytes), &returned, 0) == 0 &&
         returned == sizeof(object_bytes) && object_bytes > 0;
  }
  if (ok) {
    hash_object.resize(object_bytes);
    ok = BCryptCreateHash(algorithm, &hash, hash_object.data(), object_bytes,
                          nullptr, 0, 0) == 0;
  }
  while (ok) {
    DWORD read = 0;
    if (!ReadFile(file.value, buffer.data(),
                  static_cast<DWORD>(buffer.size()), &read, nullptr)) {
      ok = false;
      break;
    }
    if (read == 0) {
      break;
    }
    ok = BCryptHashData(hash, buffer.data(), read, 0) == 0;
  }
  if (ok) {
    ok = BCryptFinishHash(hash, output.data(),
                          static_cast<ULONG>(output.size()), 0) == 0;
  }
  SecureZeroMemory(buffer.data(), buffer.size());
  if (!hash_object.empty()) {
    SecureZeroMemory(hash_object.data(), hash_object.size());
  }
  if (hash != nullptr) {
    BCryptDestroyHash(hash);
  }
  if (algorithm != nullptr) {
    BCryptCloseAlgorithmProvider(algorithm, 0);
  }
  return ok;
}

bool HashMatchesV1(const wchar_t* path, const char* expected_lower_hex) {
  if (expected_lower_hex == nullptr || strlen(expected_lower_hex) != 64) {
    return false;
  }
  std::array<unsigned char, 32> digest{};
  if (!Sha256FileV1(path, digest)) {
    return false;
  }
  static constexpr char hex[] = "0123456789abcdef";
  std::array<char, 65> observed{};
  for (std::size_t index = 0; index < digest.size(); ++index) {
    observed[index * 2] = hex[digest[index] >> 4];
    observed[index * 2 + 1] = hex[digest[index] & 0x0f];
  }
  bool matches = memcmp(observed.data(), expected_lower_hex, 64) == 0;
  SecureZeroMemory(digest.data(), digest.size());
  SecureZeroMemory(observed.data(), observed.size());
  return matches;
}

bool ExactPinnedVersionV1(const wchar_t* path) {
  DWORD ignored = 0;
  DWORD bytes = GetFileVersionInfoSizeW(path, &ignored);
  if (bytes == 0 || bytes > 1048576) {
    return false;
  }
  std::vector<unsigned char> buffer(bytes);
  if (!GetFileVersionInfoW(path, 0, bytes, buffer.data())) {
    return false;
  }
  VS_FIXEDFILEINFO* info = nullptr;
  UINT info_bytes = 0;
  if (!VerQueryValueW(buffer.data(), L"\\",
                      reinterpret_cast<void**>(&info), &info_bytes) ||
      info == nullptr || info_bytes != sizeof(VS_FIXEDFILEINFO) ||
      info->dwSignature != VS_FFI_SIGNATURE) {
    return false;
  }
  return HIWORD(info->dwFileVersionMS) == 3 &&
         LOWORD(info->dwFileVersionMS) == 4 &&
         HIWORD(info->dwFileVersionLS) == 158 &&
         LOWORD(info->dwFileVersionLS) == 4691 &&
         HIWORD(info->dwProductVersionMS) == 3 &&
         LOWORD(info->dwProductVersionMS) == 4 &&
         HIWORD(info->dwProductVersionLS) == 158 &&
         LOWORD(info->dwProductVersionLS) == 4691;
}

bool AuthenticodeValidV1(const wchar_t* path) {
  WINTRUST_FILE_INFO file_info{};
  file_info.cbStruct = sizeof(file_info);
  file_info.pcwszFilePath = path;
  WINTRUST_DATA trust{};
  trust.cbStruct = sizeof(trust);
  trust.dwUIChoice = WTD_UI_NONE;
  trust.fdwRevocationChecks = WTD_REVOKE_NONE;
  trust.dwUnionChoice = WTD_CHOICE_FILE;
  trust.pFile = &file_info;
  trust.dwStateAction = WTD_STATEACTION_VERIFY;
  trust.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL;
  trust.dwUIContext = WTD_UICONTEXT_EXECUTE;
  GUID action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
  LONG status = WinVerifyTrust(nullptr, &action, &trust);
  trust.dwStateAction = WTD_STATEACTION_CLOSE;
  LONG close_status = WinVerifyTrust(nullptr, &action, &trust);
  return status == ERROR_SUCCESS && close_status == ERROR_SUCCESS;
}

bool JoinSiblingPathV1(const wchar_t* image_path, const wchar_t* file_name,
                       std::wstring& output) {
  const wchar_t* separator = wcsrchr(image_path, L'\\');
  if (separator == nullptr || separator == image_path) {
    return false;
  }
  output.assign(image_path,
                static_cast<std::size_t>(separator - image_path) + 1);
  output.append(file_name);
  return output.size() < kMaximumPathCharacters &&
         IsAbsoluteExistingFileV1(output.c_str());
}

bool ExactLiveMtgoIdentityV1(HANDLE process, DWORD process_id,
                             const wchar_t* bootstrap_path,
                             const wchar_t* producer_path,
                             std::uint64_t& process_start_time) {
  if (!ExactlyOneMtgoProcessV1(process_id) ||
      !IsNativeX64TargetWithExactFileNameV1(process) ||
      !ProcessStartTimeV1(process, process_start_time)) {
    return false;
  }
  wchar_t image_path[kMaximumPathCharacters] = {};
  DWORD length = static_cast<DWORD>(std::size(image_path));
  if (!QueryFullProcessImageNameW(process, 0, image_path, &length) ||
      length == 0 || length >= std::size(image_path) ||
      !IsAbsoluteExistingFileV1(image_path) ||
      !ExactPinnedVersionV1(image_path) ||
      !AuthenticodeValidV1(image_path) ||
      !HashMatchesV1(image_path, kExpectedMtgoSha256) ||
      !HashMatchesV1(bootstrap_path, kExpectedBootstrapSha256) ||
      !HashMatchesV1(producer_path, kExpectedProducerSha256)) {
    return false;
  }
  std::wstring duel_scene;
  std::wstring card;
  return JoinSiblingPathV1(image_path, L"DuelScene.dll", duel_scene) &&
         JoinSiblingPathV1(image_path, L"Card.dll", card) &&
         HashMatchesV1(duel_scene.c_str(), kExpectedDuelSceneSha256) &&
         HashMatchesV1(card.c_str(), kExpectedCardSha256);
}
#endif

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
#ifndef MTGO_LIVE_PINNED_V1
  if (!IsNativeX64TargetWithExactFileNameV1(process.value)) {
    UnmapViewOfFile(channel_view);
    return FailV1("target_not_synthetic_host");
  }
#else
  std::uint64_t pre_process_start_time = 0;
  if (!ExactLiveMtgoIdentityV1(process.value, process_id, argv[4], argv[6],
                               pre_process_start_time)) {
    UnmapViewOfFile(channel_view);
    return FailV1("live_identity_pre");
  }
#endif

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

#ifdef MTGO_LIVE_PINNED_V1
  std::uint64_t post_process_start_time = 0;
  if (!ExactLiveMtgoIdentityV1(process.value, process_id, argv[4], argv[6],
                               post_process_start_time) ||
      post_process_start_time != pre_process_start_time) {
    SecureZeroMemory(channel_view, kOutputBytes);
    UnmapViewOfFile(channel_view);
    return FailV1("live_identity_post");
  }
#endif

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
