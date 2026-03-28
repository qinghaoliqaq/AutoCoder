Classify this input into one mode. Output ONLY this JSON (no code, no explanation):
{"mode":"<chat|plan|code|debug|test>","reasoning":"<one sentence why>","refined_task":"<imperative, max 10 words>"}

Mode rules:
- chat  : greetings, questions, general conversation, anything NOT a specific dev task
- plan  : building something new (even if user says "write" or "implement")
- code  : small targeted edit, approach already decided
- debug : fixing a bug, error, or crash
- test  : writing tests or test plans

When unsure, prefer chat over plan.

Input: {{input}}
