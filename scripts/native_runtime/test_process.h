#pragma once

/* Start a program, write `input` to its stdin and wait. Return its exit code,
 * or -1 when it could not start or did not exit normally. It shares stdout and
 * stderr with the caller. No shell is involved, so we pass a path unchanged.
 * Only starting and waiting differ between platforms. */

#include <string>
#include <vector>

#if defined(_WIN32)
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

#elif defined(__APPLE__) || defined(__unix__)
#include <spawn.h>
#include <sys/wait.h>
#include <unistd.h>

extern char **environ;

inline int run_with_input(const std::vector<std::string>& arguments, const std::string& input)
{
   int pipe_ends[2];
   if (pipe(pipe_ends) != 0)
      return -1;
   posix_spawn_file_actions_t actions;
   posix_spawn_file_actions_init(&actions);
   posix_spawn_file_actions_adddup2(&actions, pipe_ends[0], STDIN_FILENO);
   posix_spawn_file_actions_addclose(&actions, pipe_ends[0]);
   posix_spawn_file_actions_addclose(&actions, pipe_ends[1]);
   std::vector<char*> argv;
   for (const std::string& argument : arguments)
      argv.push_back(const_cast<char*>(argument.c_str()));
   argv.push_back(nullptr);
   pid_t child = 0;
   const int spawned = posix_spawn(&child, argv[0], &actions, nullptr, argv.data(), environ);
   posix_spawn_file_actions_destroy(&actions);
   close(pipe_ends[0]);
   if (spawned != 0)
   {
      close(pipe_ends[1]);
      return -1;
   }
   for (size_t offset = 0; offset < input.size(); )
   {
      const ssize_t sent = write(pipe_ends[1], input.data() + offset, input.size() - offset);
      if (sent <= 0)
         break;
      offset += (size_t)sent;
   }
   close(pipe_ends[1]);
   int status = 0;
   if (waitpid(child, &status, 0) != child || !WIFEXITED(status))
      return -1;
   return WEXITSTATUS(status);
}

#else
#error "no way to start a process is declared for this platform"
#endif
