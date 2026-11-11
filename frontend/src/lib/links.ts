// Внешние ссылки студии: репозиторий, ишью, лицензии, автор и поддержка.
const REPO = "https://github.com/timoncool/dub-studio";

export const LINKS = {
  repo: REPO,
  issues: `${REPO}/issues`,
  releases: `${REPO}/releases`,
  license: `${REPO}/blob/master/LICENSE`,
  modelLicenses: `${REPO}#license`,
};

/**
 * GitHub's choice of forms (bug, idea, question) with what the studio knows passed along: the keys are the `id`s
 * of the text fields in `.github/ISSUE_TEMPLATE/*.yml`, and the chosen form fills a field from the query
 * parameter of the same name.
 */
export function issueReportUrl(version: string, gpu?: { name?: string | null; driverVersion?: string | null } | null): string {
  const query = new URLSearchParams({ version });
  const agent = navigator.userAgent;
  const os = /Windows/.test(agent) ? "Windows" : /Mac OS X|Macintosh/.test(agent) ? "macOS" : /Linux/.test(agent) ? "Linux" : "";
  if (os) query.set("os", os);
  if (gpu?.name) query.set("gpu", gpu.driverVersion ? `${gpu.name}, driver ${gpu.driverVersion}` : gpu.name);
  return `${REPO}/issues/new/choose?${query.toString()}`;
}

/** Names of timoncool's programs by the id they register with, for "taken from ...". */
export const PROGRAM_NAMES: Record<string, string> = {
  yue2: "YuE2 Studio",
  minimax: "MiniMax Music3 Studio",
  ace: "ACE-Step Studio",
  dub: "Dub Studio",
  dungeon: "Dungeon Ultimate",
  bulka: "Bulka",
};

export const DONATE = {
  boosty: "https://boosty.to/neuro_art",
  dalink: "https://dalink.to/nerual_dreming",
  github: REPO,
  telegram: "https://t.me/nerual_dreming",
  x: "https://x.com/nerualdreming",
  crypto: [["BTC", "1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC"],
           ["ETH · ERC20", "0xb5db65adf478983186d4897ba92fe2c25c594a0c"],
           ["USDT · TRC20", "TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C"]] as const,
};
