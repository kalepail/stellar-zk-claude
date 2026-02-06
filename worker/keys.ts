import { JOB_KEY_PREFIX } from "./constants";

export function jobKey(jobId: string): string {
  return `${JOB_KEY_PREFIX}${jobId}`;
}

export function tapeKey(jobId: string): string {
  return `proof-jobs/${jobId}/input.tape`;
}

export function resultKey(jobId: string): string {
  return `proof-jobs/${jobId}/result.json`;
}
