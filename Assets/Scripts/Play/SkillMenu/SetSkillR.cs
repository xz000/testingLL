using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillR : MonoBehaviour
{
    public Toggle R1;
    public Toggle R1a;
    public Toggle R1b;
    public Toggle R2;
    public Toggle R2a;
    public Toggle R2b;
    public Toggle R3;
    public Toggle R3a;
    public Toggle R3b;
    public Image IconR;
    GameObject Soldier;

    public void SetR()
    {
        AllROff();
        IconR.GetComponent<CooldownImage>().enabled = true;
        if (R1.isOn && R1a.isOn)
        {
            IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillR1>().CalcFA;
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillR1;
            return;
        }
        if (R1.isOn && R1b.isOn)
        {
            IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillR1b>().CalcFA;
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillR1b;
            return;
        }
        if (R2.isOn && R2a.isOn)
        {
            Soldier.GetComponent<SkillR2>().MyImageScript = IconR.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillR2>().enabled = true;
            return;
        }
        if (R2.isOn && R2b.isOn)
        {
            Soldier.GetComponent<SkillR2b>().MyImageScript = IconR.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillR2b>().enabled = true;
            return;
        }
        if (R3.isOn && R3a.isOn)
        {
            IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<TestSkill02>().CalcFA;
            GetComponent<SkillsLink>().KeyESkill = SkillCode.TestSkill02;
            return;
        }
        if (R3.isOn && R3b.isOn)
        {
            Soldier.GetComponent<SkillR3b>().MyImageScript = IconR.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillR3b>().enabled = true;
            return;
        }
    }

    public void AllROff()
    {
        IconR.GetComponent<CooldownImage>().enabled = false;
        Soldier.GetComponent<SkillR1>().enabled = false;
        Soldier.GetComponent<SkillR1>().MyImageScript = null;
        Soldier.GetComponent<SkillR1b>().enabled = false;
        Soldier.GetComponent<SkillR1b>().MyImageScript = null;
        Soldier.GetComponent<SkillR2>().enabled = false;
        Soldier.GetComponent<SkillR2>().MyImageScript = null;
        Soldier.GetComponent<SkillR2b>().enabled = false;
        Soldier.GetComponent<SkillR2b>().MyImageScript = null;
        Soldier.GetComponent<TestSkill02>().enabled = false;
        Soldier.GetComponent<TestSkill02>().MyImageScript = null;
        Soldier.GetComponent<SkillR3b>().enabled = false;
        Soldier.GetComponent<SkillR3b>().MyImageScript = null;
    }
}
