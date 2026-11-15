async function call<T>(path: string, method = "GET", body?: unknown): Promise<T> {
  const response = await fetch(path, { method, headers: body === undefined ? {} : { "content-type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body) })
  const answer = await response.json()
  if (!response.ok) throw new Error(answer.problem ?? response.statusText)
  return answer
}

export { call }
