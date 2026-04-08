Read the task description below and output a single line in this exact format:

PROJECT_DIR: <kebab-case-name>

Rules:
- Use only lowercase English letters, digits, and hyphens
- Reflect the business domain, not the tech stack
- 2–4 words maximum
- No spaces, no Chinese, no underscores

Examples:
  Task: 智能招聘系统，含简历解析和面试调度        → PROJECT_DIR: smart-recruitment
  Task: 餐饮外卖平台，用户下单、商家接单、骑手配送 → PROJECT_DIR: food-delivery
  Task: 苍穹外卖系统，完整餐饮外卖平台，多端功能  → PROJECT_DIR: cangqiong-food-delivery
  Task: Build a todo app with SQLite backend      → PROJECT_DIR: task-tracker
  Task: 电商平台商品管理后台                       → PROJECT_DIR: product-admin

Output exactly one line. No explanation, no markdown, nothing else.

Task: {{task}}
