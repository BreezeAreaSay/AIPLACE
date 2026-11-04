/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        void: '#0D0E12',
        panel: '#13141a',
        line: '#23242e',
        acid: '#CCFF00',
        magenta: '#FF0055',
        cyan: '#00F0FF',
      },
      fontFamily: {
        mono: ['"JetBrains Mono"', 'ui-monospace', 'SFMono-Regular', 'Menlo', 'monospace'],
      },
      boxShadow: {
        glow: '0 0 24px rgba(204,255,0,0.18)',
      },
      keyframes: {
        pulseLine: { '0%,100%': { opacity: '0.35' }, '50%': { opacity: '1' } },
      },
      animation: { pulseLine: 'pulseLine 1.4s ease-in-out infinite' },
    },
  },
  plugins: [],
}
