/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        tentative: '#64748b',
        committed: '#0f172a',
      }
    },
  },
  plugins: [],
}

