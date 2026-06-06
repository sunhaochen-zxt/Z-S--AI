#pragma once
#include "ai_reciver.h"
#include <string>

struct ai_content{
    // 角色属性
    std::string name;
    std::string personality;
    std::string background;
    std::string speaking_style;
    std::string goals;

    // 场景与时间
    std::string scene;
    std::string time;

    // 记忆与历史
    std::string memory;               // 长期记忆/状态
    std::string history_communication; // 对话历史

    // 示例对话（Few-shot / 引导语气）
    std::string example_dialogues;

    // 额外指令/高阶规则
    std::string extra_commend;

    // 环境状态追踪（好感度/位置/变量）
    std::string frankenstein_state;
};

inline std::string content_creat(const ai_content& data) {
    std::string content;

    // -----------------------------
    // System 规则：叙事和角色一致性
    // -----------------------------
    content += "[System]\n\n";
    content += "Write the next reply as the character.\n";
    content += "Remain fully in character.\n";
    content += "Maintain personality, background, goals, and speaking style consistency.\n";
    content += "Use memories and conversation history naturally.\n";
    content += "Respect the user's autonomy.\n";
    content += "Never determine the user's dialogue, thoughts, or actions.\n";
    content += "Advance the scene logically and coherently.\n";
    content += "Third-person perspective recommended for multi-character clarity.\n\n";

    // -----------------------------
    // Character 角色卡
    // -----------------------------
    content += "[Character]\n";
    content += "Name: " + data.name + "\n\n";

    content += "[Personality]\n";
    content += data.personality + "\n\n";

    content += "[Background]\n";
    content += data.background + "\n\n";

    content += "[Speaking Style]\n";
    content += data.speaking_style + "\n\n";

    content += "[Goals]\n";
    content += data.goals + "\n\n";

    // -----------------------------
    // 当前环境信息
    // -----------------------------
    content += "[Current Scene]\n";
    content += data.scene + "\n\n";

    content += "[Current Time]\n";
    content += data.time + "\n\n";

    // -----------------------------
    // 长期记忆与状态追踪
    // -----------------------------
    content += "[Memory]\n";
    content += data.memory + "\n\n";

    content += "[Conversation History]\n";
    content += data.history_communication + "\n\n";

    if (!data.frankenstein_state.empty()) {
        content += "[Frankenstein State]\n";
        content += data.frankenstein_state + "\n\n";
    }

    // -----------------------------
    // 示例对话（Few-shot 引导）
    // -----------------------------
    if (!data.example_dialogues.empty()) {
        content += "[Example Dialogues]\n";
        content += data.example_dialogues + "\n\n";
    }

    // -----------------------------
    // 额外指令/高阶叙事约束
    // -----------------------------
    if (!data.extra_commend.empty()) {
        content += "[Additional Instructions]\n";
        content += data.extra_commend + "\n\n";
    }

    return content;
}