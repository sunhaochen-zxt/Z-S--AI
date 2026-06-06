#pragma once
#include "ai_reciver.h"
#include "ai_content_creator.h"
#include <fstream>
#include <sstream>

static const char VALUE_START = '\x01';
static const char VALUE_END = '\x02';

// Escape \x01 and \x02 in value strings so they don't collide with delimiters
static std::string escape_value(const std::string& s) {
    std::string out;
    for (char c : s) {
        if (c == VALUE_START) out += "\x03\x01";
        else if (c == VALUE_END) out += "\x03\x02";
        else if (c == '\x03') out += "\x03\x03";
        else out += c;
    }
    return out;
}

static std::string unescape_value(const std::string& s) {
    std::string out;
    for (size_t i = 0; i < s.size(); i++) {
        if (s[i] == '\x03' && i + 1 < s.size()) {
            char next = s[i + 1];
            if (next == '\x01' || next == '\x02' || next == '\x03') {
                out += next;
                i++;
                continue;
            }
        }
        out += s[i];
    }
    return out;
}

static std::string make_file(const ai_content& content,
                              const question_st& question) {
    auto w = [&](const std::string& key, const std::string& val) {
        return key + '\n' + VALUE_START + escape_value(val) + VALUE_END + '\n';
    };

    std::string data;
    data += "[ai_content]\n";
    data += w("name", content.name);
    data += w("personality", content.personality);
    data += w("background", content.background);
    data += w("speaking_style", content.speaking_style);
    data += w("goals", content.goals);
    data += w("scene", content.scene);
    data += w("time", content.time);
    data += w("memory", content.memory);
    data += w("history_communication", content.history_communication);
    data += w("example_dialogues", content.example_dialogues);
    data += w("extra_commend", content.extra_commend);
    data += w("frankenstein_state", content.frankenstein_state);

    data += "[question_st]\n";
    data += w("api_key", question.api_key);
    data += w("base_url", question.base_url);
    data += w("model", question.model);
    data += w("stream", question.stream ? "true" : "false");
    data += w("reasoning_effort", question.reasoning_effort);
    data += w("thinking_type", question.extra_body.thinking_type);

    data += w("message_count", std::to_string(question.message.size()));
    for (size_t i = 0; i < question.message.size(); i++) {
        std::string p = "msg_" + std::to_string(i) + "_";
        data += w(p + "role", question.message[i].role);
        data += w(p + "content", question.message[i].content);
    }
    return data;
}

static bool save_config(const std::string& filename,
                         const ai_content& content,
                         const question_st& question) {
    std::ofstream file(filename);
    if (!file) return false;
    file << make_file(content, question);
    return file.good();
}

static bool load_config(const std::string& filename,
                         ai_content& content,
                         question_st& question) {
    std::ifstream file(filename);
    if (!file) return false;

    std::stringstream buf;
    buf << file.rdbuf();
    std::string raw = buf.str();

    auto read_val = [&](const std::string& key) -> std::string {
        std::string needle = key + '\n' + VALUE_START;
        size_t pos = raw.find(needle);
        if (pos == std::string::npos) return {};
        pos += needle.size();
        size_t end = raw.find(VALUE_END, pos);
        if (end == std::string::npos) return {};
        return unescape_value(raw.substr(pos, end - pos));
    };

    content.name               = read_val("name");
    content.personality        = read_val("personality");
    content.background         = read_val("background");
    content.speaking_style     = read_val("speaking_style");
    content.goals              = read_val("goals");
    content.scene              = read_val("scene");
    content.time               = read_val("time");
    content.memory             = read_val("memory");
    content.history_communication = read_val("history_communication");
    content.example_dialogues  = read_val("example_dialogues");
    content.extra_commend      = read_val("extra_commend");
    content.frankenstein_state = read_val("frankenstein_state");

    question.api_key           = read_val("api_key");
    question.base_url          = read_val("base_url");
    question.model             = read_val("model");
    question.stream            = read_val("stream") == "true";
    question.reasoning_effort  = read_val("reasoning_effort");
    question.extra_body.thinking_type = read_val("thinking_type");

    question.message.clear();
    std::string mc = read_val("message_count");
    if (!mc.empty()) {
        size_t n = std::stoul(mc);
        for (size_t i = 0; i < n; i++) {
            std::string p = "msg_" + std::to_string(i) + "_";
            message_st m;
            m.role    = read_val(p + "role");
            m.content = read_val(p + "content");
            question.message.push_back(m);
        }
    }
    return true;
}
