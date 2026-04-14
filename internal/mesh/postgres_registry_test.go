package mesh

import (
	"context"
	"database/sql"
	"os"
	"strings"
	"testing"
	"time"

	_ "github.com/jackc/pgx/v5/stdlib"
)

func openMeshTestDB(t *testing.T) *sql.DB {
	t.Helper()

	dsn := os.Getenv("TEAMD_TEST_POSTGRES_DSN")
	if dsn == "" {
		dsn = "postgres://postgres:postgres@localhost:5432/postgres?sslmode=disable"
	}

	db, err := sql.Open("pgx", dsn)
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	t.Cleanup(func() {
		_ = db.Close()
	})

	return db
}

func resetMeshRows(t *testing.T, db *sql.DB, agentID string) {
	t.Helper()
	statements := []string{
		`DELETE FROM mesh_agent_scores WHERE agent_id = $1`,
		`DELETE FROM mesh_agents WHERE agent_id = $1`,
	}
	for _, stmt := range statements {
		if _, err := db.Exec(stmt, agentID); err != nil {
			if strings.Contains(err.Error(), `does not exist`) {
				continue
			}
			t.Fatalf("reset mesh rows: %v", err)
		}
	}
}

func TestPostgresRegistryRegistersAndListsOnlineAgents(t *testing.T) {
	db := openMeshTestDB(t)
	reg := NewPostgresRegistry(db, 2*time.Minute)
	const agentID = "mesh-agent-a"
	resetMeshRows(t, db, agentID)

	peer := PeerDescriptor{
		AgentID: agentID,
		Addr:    "127.0.0.1:9001",
		Model:   "glm-5",
		Status:  "idle",
	}
	if err := reg.Register(context.Background(), peer); err != nil {
		t.Fatalf("register: %v", err)
	}
	if err := reg.Heartbeat(context.Background(), agentID, time.Now().UTC()); err != nil {
		t.Fatalf("heartbeat: %v", err)
	}

	online, err := reg.ListOnline(context.Background())
	if err != nil {
		t.Fatalf("list online: %v", err)
	}
	if len(online) == 0 {
		t.Fatalf("expected online peers")
	}
	var found *PeerDescriptor
	for i := range online {
		if online[i].AgentID == agentID {
			found = &online[i]
			break
		}
	}
	if found == nil {
		t.Fatalf("registered peer not found in online list: %#v", online)
	}
	if found.Status != "idle" {
		t.Fatalf("unexpected peer status: %#v", found)
	}
}

func TestPostgresRegistryRecordsTaskScores(t *testing.T) {
	db := openMeshTestDB(t)
	reg := NewPostgresRegistry(db, 2*time.Minute)
	const agentID = "mesh-agent-b"
	resetMeshRows(t, db, agentID)

	score := ScoreRecord{
		AgentID:      agentID,
		TaskClass:    "coding",
		TasksSeen:    1,
		TasksWon:     1,
		SuccessCount: 1,
		AvgLatencyMS: 125,
		LastScoreAt:  time.Now().UTC(),
	}
	if err := reg.RecordScore(context.Background(), score); err != nil {
		t.Fatalf("record score: %v", err)
	}
	scores, err := reg.ListScores(context.Background(), "coding")
	if err != nil {
		t.Fatalf("list scores: %v", err)
	}
	if len(scores) != 1 || scores[0].AgentID != agentID {
		t.Fatalf("unexpected listed scores: %#v", scores)
	}

	row := db.QueryRow(`SELECT tasks_seen, tasks_won, success_count, avg_latency_ms FROM mesh_agent_scores WHERE agent_id = $1 AND task_class = $2`, agentID, "coding")
	var tasksSeen, tasksWon, successCount int
	var avgLatencyMS int64
	if err := row.Scan(&tasksSeen, &tasksWon, &successCount, &avgLatencyMS); err != nil {
		t.Fatalf("scan score row: %v", err)
	}
	if tasksSeen != 1 || tasksWon != 1 || successCount != 1 || avgLatencyMS != 125 {
		t.Fatalf("unexpected score row: seen=%d won=%d success=%d latency=%d", tasksSeen, tasksWon, successCount, avgLatencyMS)
	}
}
