using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillT : MonoBehaviour
{
    public Toggle T1;
    public Toggle T1a;
    public Toggle T1b;
    public Toggle T2;
    public Toggle T2a;
    public Toggle T2b;
    public Toggle T3;
    public Toggle T3a;
    public Toggle T3b;
    public Image IconT;

    public void SetT()
    {
        GetComponent<SkillsLink>().KeyTSkill = null;
        if (T1.isOn && T1a.isOn)
        {
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.TestSkillLeech;
            return;
        }
        if (T1.isOn && T1b.isOn)
        {
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT1b;
            return;
        }
        if (T2.isOn && T2a.isOn)
        {
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT2;
            return;
        }
        if (T2.isOn && T2b.isOn)
        {
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT2b;
            return;
        }
        if (T3.isOn && T3a.isOn)
        {
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT3;
            return;
        }
        if (T3.isOn && T3b.isOn)
        {
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT3b;
            return;
        }
    }
}
