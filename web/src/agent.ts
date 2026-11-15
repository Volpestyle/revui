import { createAgent } from "@ui-sdk/react/agent"
import { confirm } from "@ui-sdk/react/alert-dialog"

const agent = createAgent({
  allow: (step) => (step.confirm ? confirm({ title: "Let the agent do this?", description: step.confirm, confirmLabel: "Allow" }) : true),
})

function page(tool: string, args: unknown): unknown {
  if (tool === "snapshot") return agent.snapshot()
  if (tool === "act") return agent.act(args as Parameters<typeof agent.act>[0])
  return agent.query(args as Parameters<typeof agent.query>[0])
}

export { page }
