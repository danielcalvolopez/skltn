interface CacheRingProps {
    ratio: number;
}

export function CacheRing({ ratio }: CacheRingProps) {
    const size = 120;
    const strokeWidth = 6;
    const radius = (size - strokeWidth) / 2;
    const circumference = 2 * Math.PI * radius;
    const offset = circumference * (1 - ratio);
    const percentage = Math.round(ratio * 100);

    return (
        <div className="cache-ring">
            <span className="metric-label">CACHE HIT RATIO</span>
            <svg
                width={size}
                height={size}
                viewBox={`0 0 ${size} ${size}`}
                aria-label={`Cache hit ratio: ${percentage}%`}
                role="img"
            >
                <circle
                    cx={size / 2}
                    cy={size / 2}
                    r={radius}
                    fill="none"
                    stroke="#222"
                    strokeWidth={strokeWidth}
                />
                <circle
                    cx={size / 2}
                    cy={size / 2}
                    r={radius}
                    fill="none"
                    stroke="#00ff88"
                    strokeWidth={strokeWidth}
                    strokeDasharray={circumference}
                    strokeDashoffset={offset}
                    strokeLinecap="butt"
                    transform={`rotate(-90 ${size / 2} ${size / 2})`}
                />
                <text
                    x="50%"
                    y="50%"
                    textAnchor="middle"
                    dominantBaseline="central"
                    fill="#00ff88"
                    fontSize="20"
                    fontWeight="700"
                    fontFamily="JetBrains Mono"
                >
                    {percentage}%
                </text>
            </svg>
        </div>
    );
}
