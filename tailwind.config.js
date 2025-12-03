/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
  ],
  safelist: [
    'bg-mic-idle',
    'bg-mic-listening',
    'bg-mic-processing',
    'bg-mic-error',
  ],
  theme: {
    extend: {
      animation: {
        'pulse-fast': 'pulse 1s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        'ping-slow': 'ping 1.5s cubic-bezier(0, 0, 0.2, 1) infinite',
      },
      colors: {
        'mic-idle': '#6b7280',
        'mic-listening': '#22c55e',
        'mic-processing': '#3b82f6',
        'mic-error': '#ef4444',
      },
    },
  },
  plugins: [],
}
