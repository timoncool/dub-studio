/** What the window shows of a library voice beyond its name: its card (`voices/<name>.json`) as GET /voices gives it. */
export type VoiceCardView = { display?: string; gender?: string; age?: string; lang?: string; description?: string };

let cards: Record<string, VoiceCardView> = {};

/** The cards of the library as the server last listed them; a partial listing (a download's) adds to what is known. */
export const keepVoiceCards = (next: Record<string, VoiceCardView> | undefined, whole: boolean) => {
  cards = whole ? { ...(next ?? {}) } : { ...cards, ...(next ?? {}) };
};

export const voiceCard = (name: string): VoiceCardView | undefined => cards[name];
