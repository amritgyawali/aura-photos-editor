# Using your own AI key

AURA works completely without this. Everything in the app - culling, retouching,
colour, export - runs on models that ship with it, on your own computer, with no
internet connection at all.

Adding an AI key turns on an extra layer on top of that. It makes AURA better at
a small number of judgement calls: naming an unusual ritual, choosing between two
frames that scored the same, and writing the explanations you read in the Explain
panel. You pay your AI provider directly for what it uses. AURA takes no cut and
does not see your key.

If you never add one, nothing is broken and nothing nags you.

---

## Adding a key

**Settings > AI keys.**

1. Choose your provider: Anthropic, OpenAI, Google, or your own server.
2. Paste the key.
3. Press **Save key**, then **Check**.

Check makes one tiny request - about a hundredth of a cent - and tells you which
model answered. If it fails, the message says whether the provider rejected the
key or could not be reached, which are different problems.

### Your own server

If you run Ollama, LM Studio, llama.cpp or a company gateway, choose **My own
server** and enter its address, usually `http://127.0.0.1:11434`. Nothing leaves
your network, and nothing is billed.

### Where the key is kept

In your computer's own secure key store: Windows Data Protection on Windows,
Keychain on a Mac, the login keyring on Linux. It is never written to your
catalog, never to a log file, never to a backup, and there is no screen anywhere
in AURA that can display it. Settings shows the first and last four characters so
you can tell which key is saved, and nothing in between.

If your computer refuses to store it, AURA tells you and stores it nowhere else.
A key that cannot be kept safely is a key AURA will not keep.

---

## What actually leaves your computer

Only this:

- **Small copies of photographs.** No more than 768 pixels on the long edge -
  smaller than a phone screenshot - usually arranged twelve to a sheet.
- **A short camera summary.** Camera model, lens, focal length, aperture, shutter,
  ISO, whether the flash fired, and how many seconds into the part of the day the
  frame was taken.

And nothing else. Specifically, never:

- your original files, at any size;
- your full-resolution exports;
- file names or folder names;
- where a photograph was taken;
- the couple's names, the venue, or the date;
- anything at all about a photograph's faces beyond the small picture itself.

There is an automated test in AURA's build that inspects every byte of every
upload and fails if it finds original camera data in one.

### Extra protection

**Blur faces before anything is sent.** For a client who has asked for it. Faces
are blurred in the small copy before it is encoded, so the unblurred version never
exists outside your computer.

**Cloud AI is off for every new job.** You turn it on per wedding, not once and
forget.

**Offline studio mode.** One switch, and AURA makes no outside requests of any
kind, for any job, until you turn it off. Settings then lists exactly what is
reduced.

---

## What it costs

A 3,000 photograph wedding costs about **one US dollar** at default settings.

That is not a guess. AURA asks the AI a question roughly once per forty
photographs, never once per photograph, and each question carries one sheet of
thumbnails rather than forty separate images. The build has a test that adds this
up and fails if it goes over USD 1.50.

### The spending limit

Settings shows a meter: *this wedding has used $0.42 of your $5 cap*. There are
two limits, one per wedding and one per calendar month, and both are yours to set.

The limit is checked **before** each request, using a price estimate, not
afterwards. AURA cannot overspend it.

When the limit is reached the wedding does not stop. AURA finishes with its own
models and marks those decisions in the Explain panel, so you can see exactly
which ones would have been better with more budget. Raising the limit and running
again is cheap: every answer AURA already has is reused for free.

### Why the second run is nearly free

Every answer is stored, keyed by the exact question and the exact pictures that
went with it. Re-run the same wedding and AURA asks nothing again. In testing, a
re-run reuses every answer it already had.

---

## The audit trail

Settings > AI keys lists every question AURA asked, newest first:

```
segment_naming - claude-sonnet-4-5, 2871+118 tokens, $0.01, 1840 ms
segment_naming - reused an earlier answer, no charge
segment_naming - AURA's own models (budget_reached)
```

Every AI decision anywhere in the app can be traced back to one of these lines,
and every line records what was asked, what it cost, how long it took, and how
sure the answer was. Decisions that used AURA's own models are listed too, with
the reason - those are usually the interesting ones.

---

## When the internet is not there

Nothing waits for it. A hotel with a captive portal, a venue with no signal, a
provider having a bad afternoon - AURA notices within a couple of seconds, uses
its own models for the rest of the run, and finishes the wedding. In testing, a
complete outage adds well under one per cent to the total time.

Re-run the wedding later with a working connection and those decisions are
improved. Nothing is lost in the meantime.

---

## Frequently asked

**Does my client's imagery train anyone's AI model?**
Not through AURA. AURA never sends anything for training and never opts you into
it. Whether your provider retains what you send is between you and them - their
data-retention policy is worth reading once, and AURA shows the endpoint it is
sending to so you know which policy applies.

**Can I pin the requests to one region?**
Yes. Enter your provider's regional endpoint in Settings. AURA then refuses to
send anywhere else, and records the refusal rather than quietly falling back.

**What if the AI gets something wrong?**
It cannot act on its own. Cloud reasoning only ever proposes; AURA's own
deterministic code decides and executes. A cloud answer is not allowed to overrule
a confident local decision unless it cites contradicting evidence in the photograph
- and when it does, the disagreement is recorded.

**Can I see the exact question that was asked?**
Not in the app. The audit trail stores a fingerprint of each question rather than
its text, so that the trail itself cannot become a copy of your client's details.
If you need the exact prompt for a support case, it is reproducible from the
fingerprint by AURA's support team.

**Which model does it use?**
Whichever your provider offers at the tier the job needs. Settings lists the three
it resolved. You never have to choose a model name, and when your provider ships a
new one, AURA updates the list rather than asking you to.
