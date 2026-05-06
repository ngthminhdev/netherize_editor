; Inject bash highlighting into shell commands (RUN)
((shell_command) @injection.content
  (#set! injection.language "bash")
  (#set! injection.combined))

; Inject bash into heredoc blocks
((run_instruction (heredoc_block) @injection.content)
  (#set! injection.language "bash")
  (#set! injection.include-children))
