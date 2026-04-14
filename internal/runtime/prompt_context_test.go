package runtime

import "testing"

func TestArchiveRefsForTranscript(t *testing.T) {
	refs := archiveRefsForTranscript(1001, "default", 1)
	if len(refs) != 1 || refs[0] != "archive://telegram/1001/default?messages=0-0" {
		t.Fatalf("unexpected archive refs: %#v", refs)
	}
}
