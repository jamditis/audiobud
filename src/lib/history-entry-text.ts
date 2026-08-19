export interface HistoryTextEntry {
  transcription_text: string;
  post_processed_text: string | null;
}

/** Return the text AudioBud delivered, while keeping the spoken transcript separate. */
export function historyEntryText(entry: HistoryTextEntry): string {
  return entry.post_processed_text?.trim()
    ? entry.post_processed_text
    : entry.transcription_text;
}
