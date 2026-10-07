# AIDOO Whisper

AIDOO Whisper is a Bulgarian voice interface for navigating and updating a dental patient's record in AIDOO.

## Language

**FDI tooth number**:
A two-digit dental identifier whose first digit identifies the quadrant and whose second digit identifies the tooth position. It is always spoken digit by digit, for example 18 is “едно осем”.
_Avoid_: Eighteen, whole-number pronunciation

**Dental status sequence**:
The fixed clinical traversal 18–11, 21–28, 38–31, 41–48. A status dictated from the middle of a quadrant is incomplete until the clinician addresses the preceding teeth in that quadrant.
_Avoid_: Numeric ascending order

**Visible completion**:
A clinical action is complete only when its verified result is visible in the active AIDOO browser interface. A successful backend response without an on-screen update is not visible completion.
_Avoid_: Backend success, silent completion

**Live browser presentation**:
The clinician-facing AIDOO view that reflects each navigation, read, and write action as it happens.
_Avoid_: Background sync, hidden update
