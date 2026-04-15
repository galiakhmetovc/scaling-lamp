import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { SessionSnapshot } from "../lib/types";
import { defaultSelectedTaskID, latestTaskNote, sortedPlanTasks } from "./model";

type PlanPaneProps = {
  session: SessionSnapshot | null;
  goal: string;
  task: string;
  note: string;
  selectedTaskID: string;
  onGoal: (value: string) => void;
  onTask: (value: string) => void;
  onNote: (value: string) => void;
  onSelectTask: (taskID: string) => void;
  onCreatePlan: () => void;
  onAddTask: () => void;
  onSetTaskStatus: (taskID: string, status: string) => void;
  onAddTaskNote: (taskID: string) => void;
};

export function PlanPane(props: PlanPaneProps) {
  const { session, goal, task, note, selectedTaskID, onGoal, onTask, onNote, onSelectTask, onCreatePlan, onAddTask, onSetTaskStatus, onAddTaskNote } = props;
  const plan = session?.plan;
  const tasks = plan ? sortedPlanTasks(plan) : [];
  const effectiveSelectedTaskID = selectedTaskID || (plan ? defaultSelectedTaskID(plan) : "");
  const selectedTask = tasks.find((item) => item.id === effectiveSelectedTaskID) ?? tasks[0];
  const selectedTaskNote = selectedTask && plan ? latestTaskNote(plan, selectedTask.id) : "";

  return (
    <div className="two-column">
      <section className="panel">
        <div className="section-title">
          <span>Plan</span>
          <span className="muted">{plan?.plan.id ?? "none"}</span>
        </div>
        {!plan?.plan.id ? (
          <div className="form-stack">
            <input value={goal} onChange={(event) => onGoal(event.target.value)} placeholder="Create plan goal" />
            <button onClick={onCreatePlan}>Create plan</button>
          </div>
        ) : (
          <>
            <div className="markdown-block">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{`### Goal\n${plan.plan.goal}`}</ReactMarkdown>
            </div>
            <div className="form-stack inline">
              <input value={task} onChange={(event) => onTask(event.target.value)} placeholder="Add task description" />
              <button onClick={onAddTask}>Add task</button>
            </div>
            <div className="task-list">
              {tasks.map((item) => (
                <article key={item.id} className={`task-item ${item.id === selectedTask?.id ? "active-card" : ""}`}>
                  <button className="ghost-button" onClick={() => onSelectTask(item.id)}>
                    <strong>{item.description}</strong>
                    <div className="muted">{item.id}</div>
                  </button>
                  <select value={item.status} onChange={(event) => onSetTaskStatus(item.id, event.target.value)}>
                    <option value="todo">todo</option>
                    <option value="doing">doing</option>
                    <option value="done">done</option>
                    <option value="blocked">blocked</option>
                  </select>
                </article>
              ))}
            </div>
          </>
        )}
      </section>
      <section className="panel">
        <div className="section-title">
          <span>Task details</span>
          <span className="muted">{selectedTask?.id ?? "none"}</span>
        </div>
        {selectedTask ? (
          <>
            <div className="markdown-block">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{`### ${selectedTask.description}\n\n**Status:** \`${selectedTask.status}\`\n\n**Blocked:** ${selectedTask.blocked_reason || plan?.blocked?.[selectedTask.id] || "none"}`}</ReactMarkdown>
            </div>
            {selectedTaskNote ? (
              <div className="markdown-block">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{`### Latest note\n${selectedTaskNote}`}</ReactMarkdown>
              </div>
            ) : (
              <p className="muted">No notes yet.</p>
            )}
            <div className="form-stack">
              <textarea value={note} onChange={(event) => onNote(event.target.value)} placeholder="Task note" />
              <button onClick={() => onAddTaskNote(selectedTask.id)}>Add note to {selectedTask.id}</button>
            </div>
          </>
        ) : (
          <p className="muted">No task selected yet.</p>
        )}
      </section>
    </div>
  );
}
