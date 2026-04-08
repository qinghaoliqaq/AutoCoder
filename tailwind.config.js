/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        accent: {
          claude: '#cc785c',
          codex: '#10a37f',
          director: '#7c3aed',
        },
        background: {
          light: 'rgb(250 250 249)', // zinc-50
          dark: 'rgb(9 9 11)',       // zinc-950
        },
        // CSS variable-driven semantic tokens
        surface: {
          primary: 'rgb(var(--bg-primary) / <alpha-value>)',
          secondary: 'rgb(var(--bg-secondary) / <alpha-value>)',
          tertiary: 'rgb(var(--bg-tertiary) / <alpha-value>)',
          elevated: 'rgb(var(--bg-elevated) / <alpha-value>)',
          input: 'rgb(var(--bg-input) / <alpha-value>)',
        },
        content: {
          primary: 'rgb(var(--text-primary) / <alpha-value>)',
          secondary: 'rgb(var(--text-secondary) / <alpha-value>)',
          tertiary: 'rgb(var(--text-tertiary) / <alpha-value>)',
        },
        edge: {
          primary: 'rgb(var(--border-primary) / <alpha-value>)',
          secondary: 'rgb(var(--border-secondary) / <alpha-value>)',
        },
        themed: {
          accent: 'rgb(var(--accent) / <alpha-value>)',
          'accent-soft': 'rgb(var(--accent-soft) / <alpha-value>)',
          'accent-text': 'rgb(var(--accent-text) / <alpha-value>)',
        },
      },
      fontFamily: {
        mono: ['JetBrains Mono', 'Fira Code', 'Consolas', 'monospace'],
        sans: ['Inter', 'system-ui', 'sans-serif'],
      },
      keyframes: {
        smoke: {
          '0%': { transform: 'translate(0px, 0px) scale(1) rotate(0deg)' },
          '33%': { transform: 'translate(30px, -50px) scale(1.2) rotate(120deg)' },
          '66%': { transform: 'translate(-20px, 20px) scale(0.8) rotate(240deg)' },
          '100%': { transform: 'translate(0px, 0px) scale(1) rotate(360deg)' },
        }
      },
      animation: {
        smoke: 'smoke 12s infinite cubic-bezier(0.4, 0, 0.2, 1)',
      }
    },
  },
  plugins: [
    require('@tailwindcss/typography'),
  ],
}
