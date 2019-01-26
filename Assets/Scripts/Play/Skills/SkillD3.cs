using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillD3 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance;
    public float bulletspeed = 5;
    public GameObject Missile;
    public float damage = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start ()
    {
        currentcooldown = cooldowntime;
    }
	
	// Update is called once per frame
	void GoSkillD3()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        GetComponent<DoSkill>().BeforeSkill();
        GameObject SkillTarget = FindClosestEnemy(actionplace);
        if (SkillTarget != null)
            actionplace = (Fix64Vector2)(Vector2)SkillTarget.transform.position;
        Fix64Vector2 singplace = (Fix64Vector2)(Vector2)transform.position;
        Fix64Vector2 skilldirection = actionplace - singplace;
        DoFire(singplace + (Fix64)0.76 * skilldirection.normalized(), skilldirection.normalized() * (Fix64)bulletspeed, SkillTarget);
        currentcooldown = 0;
        skillavaliable = false;
    }

    GameObject FindClosestEnemy(Fix64Vector2 findingplacef)
    {
        Vector2 findingplace = findingplacef.ToV2();
        GameObject closest = null;  // GameObject.FindWithTag("Player");
        GameObject[] Allthem = GameObject.FindGameObjectsWithTag("Player");
        float sqrdis = Mathf.Infinity;
        foreach (GameObject Him in Allthem)
        {
            if (Him == gameObject)
                continue;//跳过施法者
            Vector2 diff = (Him.GetComponent<Rigidbody2D>().position - findingplace); //距离向量
            float curDistance = diff.sqrMagnitude; //距离平方
            if (curDistance < sqrdis)
            {
                closest = Him; //更新最近距离敌人
                sqrdis = curDistance; //更新最近距离
            }
        }
        return closest;
    }

    void DoFire(Fix64Vector2 fireplace, Fix64Vector2 speed2d, GameObject target)
    {
        GameObject bullet;
        Missile.GetComponent<BombExplode>().sender = gameObject;
        bullet = Instantiate(Missile, fireplace.ToV2(), Quaternion.identity);
        bullet.GetComponent<MissileScript>().Speed = bulletspeed;
        //bullet.GetComponent<BombExplode>().bombpower = force;
        bullet.GetComponent<BombExplode>().bombdamage = damage;
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d.ToV2();
        if (target != null)
            bullet.GetComponent<MissileScript>().Target = target.GetComponent<Rigidbody2D>();
        bullet.GetComponent<BombExplode>().maxtime = maxdistance / bulletspeed;
    }

    void SkillD3SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
