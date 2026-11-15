import { Button } from "@ui-sdk/react/button"
import { Dialog, DialogBackdrop, DialogClose, DialogContent, DialogDescription, DialogTitle } from "@ui-sdk/react/dialog"
import { Radio, RadioGroup, RadioIndicator } from "@ui-sdk/react/radio"
import { toast } from "@ui-sdk/react/toast"
import { Copy, Send } from "lucide-react"
import { useState } from "react"
import { call } from "./api.ts"
import type { Data } from "./types.ts"

function Publish({ data, open, onOpenChange, onPublished }: { data: Data; open: boolean; onOpenChange: (open: boolean) => void; onPublished: () => void }) {
  const [choice, setChoice] = useState("0")
  const copies = Number(choice) === data.finish.length - 1
  const source = data.review.sources[0]
  const where = source?.repo && source.reference ? `${source.repo}${source.reference}` : "the merge request"

  async function publish() {
    if (copies) {
      await navigator.clipboard.writeText(data.preview)
      toast.success("Review text copied. Nothing was posted.")
      return onOpenChange(false)
    }
    const { ok, problem } = await call<{ ok: boolean; problem: string }>("/api/publish", "POST", { choice: Number(choice), text: data.preview })
    if (!ok) return void toast.error(`Posting hit a problem, check the merge request before retrying: ${problem}`)
    toast.success("Published to the merge request.")
    onPublished()
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogBackdrop />
      <DialogContent className="publish">
        <DialogTitle>Publish your review</DialogTitle>
        <DialogDescription>This is exactly what goes to {where}. Nothing is posted until you confirm.</DialogDescription>
        <RadioGroup aria-label="How to publish" value={choice} onValueChange={(value) => setChoice(value ?? "0")}>
          {data.finish.map((label, n) => (
            <label key={label}>
              <Radio value={String(n)}>
                <RadioIndicator />
              </Radio>
              {label}
            </label>
          ))}
        </RadioGroup>
        <pre className="preview">{data.preview}</pre>
        <footer>
          <DialogClose>Cancel</DialogClose>
          <Button data-variant="primary" data-agent-confirm={copies ? undefined : `Posts your review to ${where}, where its author and everyone on it can read it`} onClick={publish}>
            {copies ? <Copy /> : <Send />}{copies ? "Copy the text" : `Publish to ${where}`}
          </Button>
        </footer>
      </DialogContent>
    </Dialog>
  )
}

export { Publish }
