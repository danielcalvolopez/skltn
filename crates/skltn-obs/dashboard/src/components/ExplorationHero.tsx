interface ExplorationHeroProps {
    multiplier: number;
    hasSavings: boolean;
}

export function ExplorationHero({ multiplier, hasSavings }: ExplorationHeroProps) {
    const display = hasSavings ? `${multiplier.toFixed(1)}x` : '1x';
    const subtitle = hasSavings
        ? 'more codebase explored this session'
        : 'waiting for skeletonization...';

    return (
        <div className="exploration-hero">
            <span className="hero-value">{display}</span>
            <span className="hero-subtitle">{subtitle}</span>
        </div>
    );
}
