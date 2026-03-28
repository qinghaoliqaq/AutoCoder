

export const AnimatedMessageIcon = ({ className = '' }: { className?: string }) => (
    <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        className={className}
    >
        <style>
            {`
        .msg-bubble {
          stroke-dasharray: 100;
          stroke-dashoffset: 100;
          animation: draw-msg 1.5s cubic-bezier(0.175, 0.885, 0.32, 1.275) forwards;
        }
        .msg-dot {
          opacity: 0;
          animation: fade-dot 2s ease-in-out infinite;
        }
        .msg-dot-1 { animation-delay: 0.8s; }
        .msg-dot-2 { animation-delay: 1.0s; }
        .msg-dot-3 { animation-delay: 1.2s; }

        @keyframes draw-msg {
          to { stroke-dashoffset: 0; }
        }
        @keyframes fade-dot {
          0%, 100% { opacity: 0; transform: translateY(0); }
          50% { opacity: 1; transform: translateY(-2px); }
        }
      `}
        </style>
        <path
            className="msg-bubble"
            d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"
        />
        <circle cx="8" cy="12" r="1" fill="currentColor" stroke="none" className="msg-dot msg-dot-1" />
        <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" className="msg-dot msg-dot-2" />
        <circle cx="16" cy="12" r="1" fill="currentColor" stroke="none" className="msg-dot msg-dot-3" />
    </svg>
);

export const AnimatedFolderIcon = ({ className = '' }: { className?: string }) => (
    <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        className={className}
    >
        <style>
            {`
        .folder-back {
          stroke-dasharray: 80;
          stroke-dashoffset: 80;
          animation: draw-folder 1.5s cubic-bezier(0.175, 0.885, 0.32, 1) forwards;
        }
        .folder-front {
          transform-origin: bottom;
          animation: open-folder 3s cubic-bezier(0.68, -0.55, 0.265, 1.55) infinite alternate;
        }
        .folder-plus {
          transform-origin: center;
          animation: spin-plus 6s linear infinite;
        }
        
        @keyframes draw-folder {
          to { stroke-dashoffset: 0; }
        }
        @keyframes open-folder {
          0%, 20% { transform: scaleY(1) skewX(0deg); opacity: 1; }
          80%, 100% { transform: scaleY(0.85) skewX(-5deg); stroke: #8b5cf6; } 
        }
        @keyframes spin-plus {
          from { transform: rotate(0deg) scale(1); }
          50% { transform: rotate(180deg) scale(1.2); stroke: #a78bfa; }
          to { transform: rotate(360deg) scale(1); }
        }
      `}
        </style>
        <path className="folder-back" d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        <path className="folder-front" d="M2 10h20Z" />
        <path className="folder-plus" d="M12 11v6M9 14h6" />
    </svg>
);

export const AnimatedSparklesIcon = ({ className = '' }: { className?: string }) => (
    <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        className={className}
    >
        <style>
            {`
        .sparkle-center {
          transform-origin: center;
          animation: sparkle-pulse 2s cubic-bezier(0.175, 0.885, 0.32, 1.275) infinite alternate;
        }
        .sparkle-sm {
          transform-origin: center;
          animation: sparkle-float 3s ease-in-out infinite alternate;
        }
        .sparkle-sm-2 {
          transform-origin: center;
          animation: sparkle-float 2.5s ease-in-out infinite alternate-reverse;
        }
        
        @keyframes sparkle-pulse {
          0% { transform: scale(0.8) rotate(0deg); stroke-width: 1.5px; }
          100% { transform: scale(1.1) rotate(45deg); stroke-width: 2px; stroke: #fbbf24; }
        }
        @keyframes sparkle-float {
          0% { transform: translateY(0px) scale(0.8); }
          100% { transform: translateY(-4px) scale(1.2); stroke: #fcd34d; }
        }
      `}
        </style>
        <path className="sparkle-center" d="M9 3v1M9 15v1M3 9h1M15 9h1M8 8l-1-1M11 11l1 1M8 10l-1 1M11 8l1-1M12 3a6 6 0 0 0 9 9 9 9 0 0 1-9-9Z" stroke="none" fill="none" />
        <path className="sparkle-center" d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z" />
        <path className="sparkle-sm" d="M5 3v4M3 5h4" />
        <path className="sparkle-sm-2" d="M19 17v4M17 19h4" />
    </svg>
);
