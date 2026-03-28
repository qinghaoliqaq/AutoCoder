Read the task description below and output a single line in this exact format:

PROJECT_DIR: <kebab-case-name>

Rules:
- Use only lowercase English letters, digits, and hyphens
- Reflect the business domain, not the tech stack
- 2–4 words maximum
- No spaces, no Chinese, no underscores

Examples:
  Task: 智能招聘系统，含简历解析和面试调度        → PROJECT_DIR: smart-recruitment
  Task: 图库管理系统                              → PROJECT_DIR: gallery-manager
  Task: 用户认证 JWT + React 前端                 → PROJECT_DIR: jwt-auth
  Task: Build a todo app with SQLite backend      → PROJECT_DIR: todo-app
  Task: 电商平台商品管理后台                       → PROJECT_DIR: ecommerce-admin

Output exactly one line. No explanation, no markdown, nothing else.

Task: {{task}}
