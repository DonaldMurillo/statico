export interface ApiResponse<T> {
  data: T[];
  status: number;
}

export function fetchData<T>(endpoint: string): ApiResponse<T> {
  console.log(`Fetching from ${endpoint}`);
  return { data: [], status: 200 };
}
