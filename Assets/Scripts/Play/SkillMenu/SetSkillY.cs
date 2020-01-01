using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class SetSkillY : MonoBehaviour
{
    public Toggle Y1;
    public Toggle Y1a;
    public Toggle Y2;
    public Toggle Y2a;
    public Toggle Y2b;
    public Toggle Y3;
    public Toggle Y3a;
    public Toggle Y3b;
    public Toggle Y4;
    public Toggle Y4a;
    public Toggle Y4b;
    public Image IconY;

    public void SetY()
    {
        GetComponent<SkillsLink>().KeyYSkill = null;
        if (Y1.isOn)
        {
            if (Y1a.isOn)
            {
                GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY1;
                return;
            }
            GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY1b;
            return;
        }
        if (Y2.isOn)
        {
            if (Y2a.isOn)
            {
                GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY2;
                return;
            }
            GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY2b;
            return;
        }
        if (Y3.isOn)
        {
            if (Y3a.isOn)
            {
                GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY3;
                return;
            }
            GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY3b;
            return;
        }
    }
}
