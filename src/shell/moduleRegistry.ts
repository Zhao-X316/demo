// 模块注册表：加新模块（题目批改/错题统计）只往这里加一项。
export interface ModuleDef {
  key: string;
  name: string;
  icon: string;
}

export const modules: ModuleDef[] = [
  { key: "recitation", name: "背诵批改", icon: "📖" },
  { key: "exam", name: "题目批改", icon: "📝" },
  // { key: "wrongbook", name: "错题统计", icon: "📊" },
];
