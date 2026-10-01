#pragma once

#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/test_process.h is how a test starts a program on macOS and Linux; test_process.h names each platform's"
#endif

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
