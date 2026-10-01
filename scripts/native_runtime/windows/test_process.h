#pragma once

#ifndef _WIN32
#error "windows/test_process.h is how a test starts a program on Windows; test_process.h names each platform's"
#endif

#include <windows.h>

inline int run_with_input(const std::vector<std::string>& arguments, const std::string& input)
{
   /* The command line, each argument quoted, in UTF-16. */
   std::wstring command;
   for (const std::string& argument : arguments)
   {
      const int size = MultiByteToWideChar(CP_UTF8, 0, argument.c_str(), -1, nullptr, 0);
      std::wstring wide(size > 0 ? size - 1 : 0, L' ');
      if (size > 1)
         MultiByteToWideChar(CP_UTF8, 0, argument.c_str(), -1, &wide[0], size);
      command += (command.empty() ? L"\"" : L" \"") + wide + L"\"";
   }
   SECURITY_ATTRIBUTES inherit = {sizeof inherit, nullptr, TRUE};
   HANDLE read_end = nullptr, write_end = nullptr;
   if (!CreatePipe(&read_end, &write_end, &inherit, 0))
      return -1;
   SetHandleInformation(write_end, HANDLE_FLAG_INHERIT, 0);
   STARTUPINFOW startup = {};
   startup.cb = sizeof startup;
   startup.dwFlags = STARTF_USESTDHANDLES;
   startup.hStdInput = read_end;
   startup.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
   startup.hStdError = GetStdHandle(STD_ERROR_HANDLE);
   PROCESS_INFORMATION process = {};
   const BOOL started = CreateProcessW(nullptr, &command[0], nullptr, nullptr, TRUE, 0,
         nullptr, nullptr, &startup, &process);
   CloseHandle(read_end);
   if (!started)
   {
      CloseHandle(write_end);
      return -1;
   }
   DWORD written = 0;
   WriteFile(write_end, input.data(), (DWORD)input.size(), &written, nullptr);
   CloseHandle(write_end);
   WaitForSingleObject(process.hProcess, INFINITE);
   DWORD code = 0;
   GetExitCodeProcess(process.hProcess, &code);
   CloseHandle(process.hThread);
   CloseHandle(process.hProcess);
   return (int)code;
}
