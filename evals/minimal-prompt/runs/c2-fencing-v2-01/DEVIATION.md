# Protocol deviation

The solver's first three commands ran on the evaluation host instead of through the
bridge. One (`cd /opt/skill; ls -R . | head -50`) failed to change directory and listed
the first 50 entries of `evals/minimal-prompt/` on the host, which shows file and folder
names (including `tasks/*/private/`) but no file contents. The `cat` commands in the
same calls failed. It also wrote `Fencing.p`/`Fencing.pproj` to a host `/work` folder and
then piped them into the sandbox. No private file contents were read (checked in its
transcript). Treat this run's scores with that caveat.
