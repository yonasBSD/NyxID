/**
 * True when running a local development build (pnpm run ios / pnpm run android).
 * False for release builds (build:ios, build:ios:testflight, EAS).
 *
 * Set via EXPO_PUBLIC_DEV_MODE=true in the dev script (package.json).
 */
export const IS_DEV_BUILD = process.env.EXPO_PUBLIC_DEV_MODE === "true";
